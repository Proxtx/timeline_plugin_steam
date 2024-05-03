use {
    std::sync::Arc,
    base64::Engine,
    futures::StreamExt,
    mongodb::bson::doc,
    rocket::{get, routes, State},
    tokio::sync::RwLock,
    chrono::{DateTime, Duration, Utc},
    serde::{Deserialize, Serialize},
    types::{api::CompressedEvent,
    timing::TimeRange},
    crate::{Plugin as _, cache::Cache, db::{Database, Event}, PluginData}
};

#[derive(Deserialize)]
struct ConfigData {
    pub api_key: String,
    pub user_steam_id: String
}

#[derive(Deserialize, Serialize, Default)]
struct LastGameCache {
    pub last_game: Option<(Game, DateTime<Utc>)>
}

pub struct Plugin {
    plugin_data: PluginData,
    config: ConfigData,
    cache: RwLock<Cache<LastGameCache>>
}

impl crate::Plugin for Plugin {
    async fn new(data: PluginData) -> Self
        where
            Self: Sized {
        let config: ConfigData = toml::Value::try_into(
            data.config
                .clone().expect("Failed to init steam plugin! No config was provided!")
                ,
        )
        .unwrap_or_else(|e| panic!("Unable to init steam plugin! Provided config does not fit the requirements: {}", e));

        let cache: Cache<LastGameCache> =
            Cache::load::<Plugin>().await.unwrap_or_else(|e| {
                panic!(
                    "Failed to init media_scan plugin! Unable to load cache: {}",
                    e
                )
            });
        Plugin { plugin_data: data, config, cache: RwLock::new(cache) }
    }

    fn get_type() -> types::api::AvailablePlugins
        where
            Self: Sized {
        types::api::AvailablePlugins::timeline_plugin_steam
    }

    fn request_loop<'a>(
            &'a self,
        ) -> std::pin::Pin<Box<dyn futures::Future<Output = Option<chrono::Duration>> + Send + 'a>> {
        Box::pin(async move {
            if let Err(e) = self.update_playing_status().await {
                self.plugin_data.report_error_string(format!("Unable to update playing status: {}", e))
            }

            Some(Duration::try_seconds(30).unwrap())
        })
    }

    fn get_compressed_events(
            &self,
            query_range: &types::timing::TimeRange,
        ) -> std::pin::Pin<Box<dyn futures::Future<Output = types::api::APIResult<Vec<types::api::CompressedEvent>>> + Send>> {
        let filter = Database::combine_documents(Database::generate_find_plugin_filter(Plugin::get_type()), Database::combine_documents(Database::generate_range_filter(query_range), doc! {
                "event.event_type": "Game"
        }));
        let database = self.plugin_data.database.clone();
        Box::pin(async move { 
            let mut cursor = database
                .get_events::<GameSave>()
                .find(filter, None)
                .await?;
            let mut result = Vec::new();
            while let Some(v) = cursor.next().await {
                let t = v?;
                result.push(CompressedEvent {
                    title: t.event.game.name.clone(),
                    time: t.timing,
                    data: Box::new(t.event.game),
                })
            }

            Ok(result)
        })
    }

    fn get_routes() -> Vec<rocket::Route>
        where
            Self: Sized, {
        routes![get_cover]
    }
}

impl Plugin {
    pub async fn update_playing_status(&self) ->  Result<(), String> {
        enum Action {
            SaveGame(Game, TimeRange),
            Nothing
        }
        let mut action = Action::Nothing;

        let game = self.get_current_game().await?;
        if let Err(e) = self.cache.write().await.modify::<Plugin>(|cache| {
            match (&cache.last_game, game) {
                (Some(_game_start), None) => {
                    let game_start = cache.last_game.take().unwrap(); //we just tested for this
                    action = Action::SaveGame(game_start.0, TimeRange { start: game_start.1, end: Utc::now() });
                }
                (None, Some(current_game)) => {
                    cache.last_game = Some((current_game, Utc::now()));
                    action = Action::Nothing
                }
                (Some(last_game), Some(current_game)) => {
                    if last_game.0 != current_game {
                        cache.last_game = Some((current_game, Utc::now()));
                    }
                    action = Action::Nothing
                }
                (_, _) => {
                    action = Action::Nothing
                }
            }
        }) {
            return Err(format!("Unable to read/write cache: {}", e));
        }

        if let Action::SaveGame(game, range) = action {
            self.check_or_insert_game_cover(&game).await?;
            match self.plugin_data.database.register_single_event(&Event { id: format!("{}@{}", game.id, serde_json::to_string(&range).unwrap()), timing: types::timing::Timing::Range(range), plugin: Plugin::get_type(), event: GameSave {
                game, 
                event_type: EventType::Game
            } }).await {
                Ok(_) => {},
                Err(e) => {
                    return Err(format!("Unable to register game: {}", e));
                }
            }
        }

        Ok(())
    }

    pub async fn check_or_insert_game_cover (&self, game: &Game) -> Result<(), String>{
        let cnt = self.plugin_data.database.get_events::<CoverSave>().count_documents(Database::combine_documents(Database::generate_find_plugin_filter(Plugin::get_type()), doc! {
            "event.event_type": "Cover",
            "event.game_id": game.id.clone()
        }), None).await;

        let cnt = match cnt {
            Ok(v) => v,
            Err(e) => {
                return Err(format!("Error getting current cover: {}", e));
            }
        };

        if cnt > 0 {
            return Ok(());
        }
        
        let buffer = match reqwest::get(format!("https://cdn.cloudflare.steamstatic.com/steam/apps/{}/header.jpg", game.id)).await {
            Ok(v) => {
                match v.bytes().await {
                    Ok(v) => {
                        base64::prelude::BASE64_STANDARD.encode(v)
                    }
                    Err(e) => {
                        return Err(format!("Unable to read game cover response: {}", e));
                    }
                }
            }
            Err(e) => {
                return Err(format!("Unable to fetch game cover: {}", e));
            }
        };
        match self.plugin_data.database.register_single_event(&Event { timing: types::timing::Timing::Instant(Utc::now()), id: game.id.clone(), plugin: Plugin::get_type(), event: CoverSave {
            data: buffer,
            game_id: game.id.clone(),
            event_type: EventType::Cover
        }}).await {
            Ok(_v) => {},
            Err(e) => {
                return Err(format!("Unable to save game cover in database: {}", e));
            }
        }

        Ok(())
    }

    pub async fn get_current_game(&self) -> Result<Option<Game>, String> {
        let res = reqwest::get(format!("https://api.steampowered.com/ISteamUser/GetPlayerSummaries/v0002/?key={}&steamids={}", self.config.api_key, self.config.user_steam_id)).await;
        let res = match res {
            Ok(v) => match v.text().await {
                Ok(v) => v,
                Err(e) => {
                    return Err(format!("Unable to read steam api response: {}", e));
                }
            },
            Err(e) => {
                return Err(format!("Unable to fetch game: {}", e));
            }
        };

        let parsed_res: SteamInfoRes = match serde_json::from_str(&res) {
            Ok(v) => {
                v
            }
            Err(e) => {
                return Err(format!("Unable to parse steam response: {}", e));
            }
        };

        let (name, id) = match (&parsed_res.response.players[0].gameextrainfo, &parsed_res.response.players[0].gameid) {
            (Some(name), Some(id)) => {
                (name.clone(), id.clone())
            }
            _ => {
                return Ok(None);
            }
        };

        Ok(Some(Game { name, id }))
    }
}

#[get("/<game_id>")]
async fn get_cover(database: &State<Arc<Database>>, game_id: &str) -> Option<Vec<u8>> {
    match database.get_events::<CoverSave>().find_one(Database::combine_documents(Database::generate_find_plugin_filter(Plugin::get_type()), doc! {
        "event.event_type": "Cover",
        "event.game_id": game_id
    }), None).await {
        Ok(Some(v)) => {
            Some(base64::prelude::BASE64_STANDARD.decode(v.event.data).unwrap())
        }
        _ => {
            None
        }
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
//i know this is shit
enum EventType {
    Game,
    Cover
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct CoverSave {
    data: String,
    game_id: String,
    event_type: EventType
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct GameSave {
    game: Game, 
    event_type: EventType
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct Game {
    name: String,
    id: String
}

#[derive(Deserialize)]
struct SteamInfoRes {
    pub response: SteamInfoResResponse
}

#[derive(Deserialize)]
struct SteamInfoResResponse {
    pub players: Vec<SteamInfoResResponsePlayer>
}

#[derive(Deserialize)]
struct SteamInfoResResponsePlayer {
    pub gameid: Option<String>,
    pub gameextrainfo: Option<String>
}
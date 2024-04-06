use std::str::FromStr;
use std::sync::RwLock;

use serde::{Deserialize, Serialize};
use url::Url;

use crate::cache::Cache;
use crate::PluginData;
use crate::config::Config;

#[derive(Deserialize)]
struct ConfigData {
    pub api_key: String,
    pub user_steam_id: String
}

#[derive(Deserialize, Serialize, Default)]
struct LastGameCache {
    pub last_game_id: String
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
                .clone().expect("Failed to init spotify plugin! No config was provided!")
                ,
        )
        .unwrap_or_else(|e| panic!("Unable to init spotify plugin! Provided config does not fit the requirements: {}", e));

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
        
    }

    fn get_compressed_events(
            &self,
            query_range: &types::timing::TimeRange,
        ) -> std::pin::Pin<Box<dyn futures::Future<Output = types::api::APIResult<Vec<types::api::CompressedEvent>>> + Send>> {
        Box::pin(async move {
            Ok(Vec::new())
        })
    }
}

impl Plugin {
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

struct Game {
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
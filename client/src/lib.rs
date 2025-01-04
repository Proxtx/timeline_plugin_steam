use { 
    client_api::{api, plugin::{PluginData, PluginEventData, PluginTrait}, result::EventResult, style::Style}, leptos::{view, IntoView, View}, serde::{Deserialize, Serialize}
};

pub struct Plugin {
    #[allow(unused)]
    plugin_data: PluginData,
}

impl PluginTrait for Plugin {
    async fn new(data: PluginData) -> Self
        where
            Self: Sized {
            Plugin {
                plugin_data: data
            }
    }

    fn get_component(&self, data: PluginEventData) -> EventResult<Box<dyn FnOnce() -> leptos::View>> {
        let data = data.get_data::<Game>()?;
        Ok(Box::new(move || -> View {
            view! {
                <div style="display: flex; flex-direction: row; width: 100%; gap: calc(var(--contentSpacing) * 0.5); background-color: var(--accentColor2);align-items: start;">
                    <img
                        style="height: calc(var(--contentSpacing) * 8);"
                        src=move || {
                            api::relative_url("/api/plugin/timeline_plugin_steam/")
                                .unwrap()
                                .join(&data.id)
                                .unwrap()
                                .to_string()
                        }
                    />

                    <div style="padding-top: calc(var(--contentSpacing) * 0.5); padding-bottom: calc(var(--contentSpacing) * 0.5); color: var(--lightColor); overflow: hidden;">
                        <h3>{move || { data.name.clone() }}</h3>
                    </div>
                </div>
            }.into_view()
        }))
    }

    fn get_style(&self) -> Style {
        Style::Acc2
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Game {
    pub name: String,
    pub id: String,
}
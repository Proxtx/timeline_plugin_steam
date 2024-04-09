use { 
    leptos::{view, IntoView, View}, 
    serde::{Deserialize, Serialize},
    crate::plugin_manager::PluginData
};

pub struct Plugin {
    #[allow(unused)]
    plugin_data: PluginData,
}

impl crate::Plugin for Plugin {
    async fn new(data: crate::plugin_manager::PluginData) -> Self
        where
            Self: Sized {
            Plugin {
                plugin_data: data
            }
    }

    fn get_component(&self, data: crate::plugin_manager::PluginEventData) -> crate::event_manager::EventResult<Box<dyn FnOnce() -> leptos::View>> {
        let data = data.get_data::<Game>()?;
        Ok(Box::new(move || -> View {
            view! {
                <div style="display: flex; flex-direction: row; width: 100%; gap: calc(var(--contentSpacing) * 0.5); background-color: var(--accentColor2);align-items: start;">
                    <img
                        style="height: calc(var(--contentSpacing) * 8);"
                        src=move || {
                            crate::api::relative_url("/api/plugin/timeline_plugin_steam/")
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

    fn get_style(&self) -> crate::plugin_manager::Style {
        crate::plugin_manager::Style::Acc2
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct Game {
    pub name: String,
    pub id: String,
}
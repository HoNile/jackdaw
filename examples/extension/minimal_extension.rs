use bevy::prelude::*;
use jackdaw::prelude::*;

#[derive(Default)]
pub struct MyExtension;

impl JackdawExtension for MyExtension {
    fn id(&self) -> String {
        "my_extension".to_string()
    }

    fn register(&self, _ctx: &mut ExtensionContext) {
        info!("The custom extension has been registered! How cool is that!");
    }
}

fn main() -> AppExit {
    App::new()
        .add_plugins((
            DefaultPlugins,
            EditorPlugins::default()
                .set(ExtensionPlugin::default().with_extension::<MyExtension>()),
        ))
        .run()
}

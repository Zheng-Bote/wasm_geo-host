use flagme_plugin_weather_enrich::WeatherEnrichPlugin;
use flagme_sdk::Plugin;

fn main() {
    let plugin = WeatherEnrichPlugin::new();
    println!("Plugin {} v{}", plugin.name(), plugin.version());
}

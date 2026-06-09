use toml::Value;

pub fn generate_configuration_from_conf_file(config_file: String) {
    log::info!("Generating configuration for {}.", config_file);

    let content = std::fs::read_to_string(config_file).expect("Failed to read config file: {}");

    let value: Value = content.parse().expect("Failed to parse config file: {}");
}

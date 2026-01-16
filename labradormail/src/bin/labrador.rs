use std::collections::HashSet;
use std::process;

use labradormail::prelude::run_from_servers;
use labradormail::prelude::ServerConfig;
use rc_conf::RcConf;
use rc_conf::SwitchPosition;

#[tokio::main]
async fn main() {
    let rc_conf = match RcConf::parse("labrador.conf") {
        Ok(rc) => rc,
        Err(e) => {
            eprintln!("Error parsing labrador.conf: {:?}", e);
            process::exit(1);
        }
    };

    // Extract service names from variables ending in _ENABLED, _BASEURL, or _MERGE.
    let services: HashSet<String> = rc_conf
        .variables()
        .iter()
        .filter_map(|var| {
            var.strip_suffix("_ENABLED")
                .or_else(|| var.strip_suffix("_BASEURL"))
                .or_else(|| var.strip_suffix("_MERGE"))
                .map(|s| s.to_string())
        })
        .collect();

    // Build ServerConfig for each enabled service.
    let configs: Vec<ServerConfig> = services
        .iter()
        .filter(|service| rc_conf.service_switch(service) == SwitchPosition::Yes)
        .filter_map(|service| {
            let base_url = rc_conf.lookup_suffix(service, "BASEURL")?;
            let merge = rc_conf
                .lookup_suffix(service, "MERGE")
                .map(|v| v == "YES" || v == "yes" || v == "true" || v == "1")
                .unwrap_or(false);
            Some(ServerConfig::new(service.clone(), base_url, merge))
        })
        .collect();

    if configs.is_empty() {
        eprintln!("Error: No servers configured with BASEURL in labrador.conf");
        process::exit(1);
    }

    if let Err(e) = run_from_servers(&configs).await {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}

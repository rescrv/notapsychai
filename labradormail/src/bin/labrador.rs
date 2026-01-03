use std::collections::HashSet;
use std::process;

use labradormail::run_from_servers;
use rc_conf::RcConf;
use rc_conf::SwitchPosition;

fn main() {
    let rc_conf = match RcConf::parse("labrador.conf") {
        Ok(rc) => rc,
        Err(e) => {
            eprintln!("Error parsing labrador.conf: {:?}", e);
            process::exit(1);
        }
    };

    // Extract service names from variables ending in _ENABLED or _BASEURL.
    let services: HashSet<String> = rc_conf
        .variables()
        .iter()
        .filter_map(|var| {
            var.strip_suffix("_ENABLED")
                .or_else(|| var.strip_suffix("_BASEURL"))
                .map(|s| s.to_string())
        })
        .collect();

    // Collect base URLs from enabled services.
    let base_urls: Vec<String> = services
        .iter()
        .filter(|service| rc_conf.service_switch(service) == SwitchPosition::Yes)
        .filter_map(|service| rc_conf.lookup_suffix(service, "BASEURL"))
        .collect();

    if base_urls.is_empty() {
        eprintln!("Error: No servers configured with BASEURL in labrador.conf");
        process::exit(1);
    }

    if let Err(e) = run_from_servers(&base_urls) {
        eprintln!("Error: {}", e);
        process::exit(1);
    }
}

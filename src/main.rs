#![feature(ip)]

use std::{default::Default, fs, net::IpAddr, path::PathBuf};

use anyhow::{Context, Result};
use clap::{command, Arg, ArgAction, ArgGroup, Id};
use dns_lookup::lookup_addr;
use geoip2::{City, Reader, ASN};
use serde::Serialize;

const ASN_IP2: &str = "GeoIP2-ASN.mmdb";
const ASN_LITE2: &str = "GeoLite2-ASN.mmdb";
const CITY_IP2: &str = "GeoIP2-City.mmdb";
const CITY_LITE2: &str = "GeoLite2-City.mmdb";

#[derive(Serialize)]
struct IpInfo {
    ip: String, // "142.250.203.110"
    #[serde(skip_serializing_if = "Option::is_none")]
    hostname: Option<String>, // "zrh04s16-in-f14.1e100.net"
    #[serde(skip_serializing_if = "Option::is_none")]
    city: Option<String>, // "Zürich"
    #[serde(skip_serializing_if = "Option::is_none")]
    region_iso: Option<String>, // "ZH"
    #[serde(skip_serializing_if = "Option::is_none")]
    country_iso: Option<String>, // "CH"
    #[serde(skip_serializing_if = "Option::is_none")]
    long: Option<f64>, // 47.3667
    #[serde(skip_serializing_if = "Option::is_none")]
    lat: Option<f64>, // 8.5500
    #[serde(skip_serializing_if = "Option::is_none")]
    osm: Option<String>, // "https://openstreetmap.org/#map=11/47.3667/8.5500"
    #[serde(skip_serializing_if = "Option::is_none")]
    org: Option<String>, // "AS15169 Google LLC"
    #[serde(skip_serializing_if = "Option::is_none")]
    postal: Option<String>, // "8000"
    #[serde(skip_serializing_if = "Option::is_none")]
    timezone: Option<String>, // "Europe/Zurich"
}

// Not routed
#[derive(Serialize)]
struct IpBogon {
    ip: String,
    bogon: bool,
}

impl Default for IpBogon {
    fn default() -> Self {
        Self {
            ip: String::from(""),
            bogon: true,
        }
    }
}

fn main() -> Result<()> {
    // Define cmdl interface properties.
    let matches = command!()
        .arg(
            Arg::new("mmdbdir")
                .short('m')
                .long("mmdir")
                .help("Directory containing mmdb files")
                .default_value("/var/lib/GeoIP")
                .value_parser(clap::value_parser!(PathBuf)),
        )
        .arg(
            Arg::new("ipaddr")
                .required(true)
                .help("[-m <mmdbdir>] [--lang <langcode>] [--last] Query geoip info")
                .value_parser(clap::value_parser!(IpAddr)),
        )
        .arg(
            Arg::new("langcode")
                .long("lang")
                .help("IETF language code used to query names")
                .default_value("en"),
        )
        .arg(
            Arg::new("last_subdiv")
                .long("last")
                .help("For region details read last subdivision rather than first")
                .action(ArgAction::SetTrue),
        )
        .next_help_heading("Metainfo only") // Structure help in a slightly clearer way.
        .arg(
            Arg::new("list_languages")
                .long("ll")
                .help("[-m <mmdbdir>] List IETF language codes applicable for City DB and exit")
                .action(ArgAction::SetTrue),
        )
        .group(
            ArgGroup::new("lookup")
                .args(["ipaddr", "langcode", "last_subdiv"])
                .multiple(true)
                .conflicts_with("listonly"),
        )
        .group(
            ArgGroup::new("listonly")
                .arg("list_languages")
                .conflicts_with("lookup"),
        )
        .get_matches();

    // Get DB directory.
    let Some(dir) = matches.get_one::<PathBuf>("mmdbdir") else { panic!("required") };
    let dir = dir.to_string_lossy();

    // Initialize City DB reader.
    let buffer = match fs::read(format!("{}/{}", dir, CITY_IP2)) {
        Ok(bf) => bf,
        Err(_) => fs::read(format!("{}/{}", dir, CITY_LITE2))
            .ok()
            .with_context(|| format!("Failed to read City mmdb in {dir}"))?,
    };
    let rdr_city = Reader::<City>::from_bytes(&buffer)
        .ok()
        .context("Failed to create City reader")?;
    //eprintln!("{:?}", rdr_city.get_metadata());

    if matches.get_one::<Id>("listonly").is_some() {
        // Implementation details for metadata
        let languages = &rdr_city.get_metadata().languages;
        languages
            .iter()
            .enumerate()
            .map(|(i, lang)| {
                if i < languages.len() - 1 {
                    print!("{lang}, ")
                } else {
                    println!("{lang}")
                }
            })
            .for_each(drop); // As we just want to print, we need no result.
    } else if matches.get_one::<Id>("lookup").is_some() {
        // Implementation details for lookup
        let Some(ipaddr) = matches.get_one::<IpAddr>("ipaddr") else { panic!("required") };
        let Some(lang_code) = matches.get_one::<String>("langcode") else { panic!("required") };
        let Some(last_subdiv) = matches.get_one::<bool>("last_subdiv") else { panic!("required") };

        /*
         * If not globally routable, we make it real quick.
         *
         * (Using `.is_global()` depends on a nightly feature in IpAddr which has to be imported as `feature(ip)`.)
         */
        if !ipaddr.is_global() {
            println!(
                "{}",
                serde_json::to_string_pretty(&IpBogon {
                    ip: ipaddr.to_string(),
                    ..Default::default()
                })?
            );

            return Ok(());
        }

        // Initialize ASN DB reader.
        let buffer = match fs::read(format!("{}/{}", dir, ASN_IP2)) {
            Ok(bf) => bf,
            Err(_) => fs::read(format!("{}/{}", dir, ASN_LITE2))
                .ok()
                .with_context(|| format!("Failed to read ASN mmdb in {}", dir))?,
        };
        let rdr_asn = Reader::<ASN>::from_bytes(&buffer)
            .ok()
            .context("Failed to create ASN reader")?;
        //eprintln!("{:?}", rdr_asn.get_metadata());

        // Get geo entry for ip address on city level.
        let geo = rdr_city
            .lookup(*ipaddr)
            .ok()
            .context("Failed to query City mmdb")?;

        // Get ASN entry for ip address.
        let org = rdr_asn
            .lookup(*ipaddr)
            .ok()
            .context("Failed to query ASN mmdb")?;

        // Prepare location data.
        let long;
        let lat;
        let osm;

        if let Some(location) = get_some_loc(&geo) {
            let (longitude, latitude) = location;
            long = Some(longitude);
            lat = Some(latitude);
            osm = Some(format!(
                "https://openstreetmap.org/#map=11/{longitude}/{latitude}"
            ));
        } else {
            long = None;
            lat = None;
            osm = None;
        }

        // Serialize as JSON and write.
        println!(
            "{}",
            serde_json::to_string_pretty(&IpInfo {
                ip: ipaddr.to_string(),
                hostname: lookup_addr(&ipaddr).ok(),
                city: get_some_city(&geo, lang_code),
                region_iso: get_some_region_iso(&geo, *last_subdiv),
                country_iso: get_some_country_iso(&geo),
                long,
                lat,
                osm,
                org: get_some_org(org),
                postal: get_some_postal(&geo),
                timezone: get_some_tz(&geo)
            })?
        );
    } else {
        panic!("fresh out of arg group matches") // Never happens, unless cmdl interface is
                                                 // changed.
    }

    Ok(())
}

fn get_some_city(geo: &City, lang_code: &str) -> Option<String> {
    if let Some(city) = geo.city.as_ref() {
        city.names
            .as_ref()
            // TODO:    better than unwrap() !
            //          Maybe don't always take `en`.
            .map(|map| {
                //eprintln!("{:#?}", map.key());
                map.get(lang_code).unwrap().to_string()
            })
    } else {
        None
    }
}

fn get_some_region_iso(geo: &City, last: bool) -> Option<String> {
    let subdivs = geo.subdivisions.as_ref();

    subdivs.and_then(|subdiv| {
        let subdiv = if last { subdiv.last() } else { subdiv.first() };
        match subdiv {
            Some(subdiv) => subdiv.iso_code.map(|code| code.to_string()),
            _ => None,
        }
    })
}

fn get_some_country_iso(geo: &City) -> Option<String> {
    if let Some(country) = geo.country.as_ref() {
        country.iso_code.map(String::from)
    } else {
        None
    }
}

fn get_some_loc(geo: &City) -> Option<(f64, f64)> {
    if let Some(loc) = geo.location.as_ref() {
        match (loc.latitude, loc.longitude) {
            (Some(lat), Some(long)) => Some((lat, long)),
            _ => None,
        }
    } else {
        None
    }
}

fn get_some_org(asn: ASN) -> Option<String> {
    match (
        asn.autonomous_system_number,
        asn.autonomous_system_organization,
    ) {
        (Some(number), Some(organization)) => Some(format!("AS{number} {organization}")),
        (Some(number), None) => Some(format!("AS{number}")),
        (None, Some(organization)) => Some(format!("{organization}")),
        (None, None) => None,
    }
}

fn get_some_postal(geo: &City) -> Option<String> {
    if let Some(postal) = geo.postal.as_ref() {
        postal.code.map(String::from)
    } else {
        None
    }
}

fn get_some_tz(geo: &City) -> Option<String> {
    if let Some(location) = geo.location.as_ref() {
        location.time_zone.map(String::from)
    } else {
        None
    }
}

#![feature(ip)]

use std::{default::Default, fs, net::IpAddr};

use anyhow::{Context, Result};
use argh::FromArgs;
use dns_lookup::lookup_addr;
use geoip2::{City, Reader, ASN};
use serde::Serialize;

const ASN_IP2: &str = "GeoIP2-ASN.mmdb";
const ASN_LITE2: &str = "GeoLite2-ASN.mmdb";
const CITY_IP2: &str = "GeoIP2-City.mmdb";
const CITY_LITE2: &str = "GeoLite2-City.mmdb";

#[derive(FromArgs)]
/// Emulate the behaviour of `curl ipinfo.io/<ipaddr>`.
struct CmdlArgs {
    /// ipaddr
    #[argh(positional)]
    ipaddr: IpAddr,

    /// directory containing GeoIP2/GeoLite2 files
    #[argh(option, short = 'm', default = "String::from(\"/var/lib/GeoIP\")")]
    //#[argh(option, default = "defaultdir()")]
    mmdir: String,
}

// anything public
#[derive(Serialize)]
struct IpInfo {
    ip: String, // "142.250.203.110"
    #[serde(skip_serializing_if = "Option::is_none")]
    hostname: Option<String>, // "zrh04s16-in-f14.1e100.net"
    #[serde(skip_serializing_if = "Option::is_none")]
    city: Option<String>, // "Zürich"
    #[serde(skip_serializing_if = "Option::is_none")]
    region: Option<String>, // "Zurich"
    #[serde(skip_serializing_if = "Option::is_none")]
    country: Option<String>, // "CH"
    #[serde(skip_serializing_if = "Option::is_none")]
    loc: Option<String>, // "47.3667,8.5500"
    #[serde(skip_serializing_if = "Option::is_none")]
    org: Option<String>, // "AS15169 Google LLC"
    #[serde(skip_serializing_if = "Option::is_none")]
    postal: Option<String>, // "8000"
    #[serde(skip_serializing_if = "Option::is_none")]
    timezone: Option<String>, // "Europe/Zurich"
}

// not routed
#[derive(Serialize)]
struct IpBogon {
    ip: String,
    loc: f64,
    bogon: bool,
}

impl Default for IpBogon {
    fn default() -> Self {
        Self {
            ip: String::from(""),
            loc: 48.0,
            bogon: true,
        }
    }
}

fn main() -> Result<()> {
    // Parse command line arguments.
    let args: CmdlArgs = argh::from_env();

    /*
       IF not globally routable, we make it real quick.

       (Using `.is_global()` depends on a nightly feature in IpAddr which has to be imported as `feature(ip)`.)
    */
    if !args.ipaddr.is_global() {
        println!(
            "{}",
            serde_json::to_string_pretty(&IpBogon {
                ip: args.ipaddr.to_string(),
                ..Default::default()
            })?
        );

        return Ok(());
    }

    // Set up readers for maxmind database files.
    let buffer = match fs::read(format!("{}/{}", &args.mmdir, ASN_IP2)) {
        Ok(bf) => bf,
        Err(_) => fs::read(format!("{}/{}", &args.mmdir, ASN_LITE2))
            .ok()
            .with_context(|| format!("Failed to read ASN mmdb in {}", args.mmdir))?,
    };
    let rdr_asn = Reader::<ASN>::from_bytes(&buffer)
        .ok()
        .context("Failed to create ASN reader")?;

    let buffer = match fs::read(format!("{}/{}", &args.mmdir, CITY_IP2)) {
        Ok(bf) => bf,
        Err(_) => fs::read(format!("{}/{}", &args.mmdir, CITY_LITE2))
            .ok()
            .with_context(|| format!("Failed to read City mmdb in {}", args.mmdir))?,
    };
    let rdr_city = Reader::<City>::from_bytes(&buffer)
        .ok()
        .context("Failed to create City reader")?;

    let org = rdr_asn
        .lookup(args.ipaddr)
        .ok()
        .context("Failed to query ASN mmdb")?;

    let geo = rdr_city
        .lookup(args.ipaddr)
        .ok()
        .context("Failed to query City mmdb")?;

    // Serialize as JSON and write.
    println!(
        "{}",
        serde_json::to_string_pretty(&IpInfo {
            ip: args.ipaddr.to_string(),
            hostname: lookup_addr(&args.ipaddr).ok(),
            city: get_some_city(&geo),
            region: get_some_region(&geo),
            country: get_some_country(&geo),
            loc: get_some_loc(&geo),
            org: get_some_org(org),
            postal: get_some_zip(&geo),
            timezone: get_some_tz(&geo)
        })?
    );

    Ok(())
}

fn get_some_city(geo: &City) -> Option<String> {
    if let Some(city) = geo.city.as_ref() {
        match &city.names {
            // TODO:    better than unwrap() !
            //          Maybe don't always take `en`.
            Some(map) => Some(map.get("en").unwrap().to_string()),
            _ => None,
        }
    } else {
        None
    }
}

fn get_some_region(geo: &City) -> Option<String> {
    geo.subdivisions
        .as_ref()
        // TODO:    better than unwrap() !
        //          Make choice of first() / last() configurable.
        .map(|subdiv| subdiv.last().unwrap().iso_code.unwrap().to_string())
}

fn get_some_country(geo: &City) -> Option<String> {
    if let Some(country) = geo.country.as_ref() {
        country.iso_code.map(String::from)
    } else {
        None
    }
}

fn get_some_loc(geo: &City) -> Option<String> {
    geo.location.as_ref().map(|loc| {
        format!(
            "{},{}",
            // TODO: better than unwrap() !
            loc.latitude.unwrap(),
            loc.longitude.unwrap()
        )
    })
}

fn get_some_org(asn: ASN) -> Option<String> {
    // TODO: can we do better than unwrap() ?
    match asn {
        ASN {
            autonomous_system_number: None,
            autonomous_system_organization: None,
        } => None,

        ASN {
            autonomous_system_number,
            autonomous_system_organization: None,
        } => Some(format!("AS{}", autonomous_system_number.unwrap())),

        ASN {
            autonomous_system_number: None,
            autonomous_system_organization,
        } => Some(format!("{}", autonomous_system_organization.unwrap())),

        ASN {
            autonomous_system_number,
            autonomous_system_organization,
        } => Some(format!(
            "AS{} {}",
            autonomous_system_number.unwrap(),
            autonomous_system_organization.unwrap()
        )),
    }
}

fn get_some_zip(geo: &City) -> Option<String> {
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

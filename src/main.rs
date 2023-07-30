mod osm2network;

use std::collections::HashMap;
use std::time::Instant;

use anyhow::Result;
use clap::Parser;
use futures::{stream, StreamExt};
use geojson::{GeoJson, Value};
use indicatif::{HumanCount, ProgressBar, ProgressStyle};
use reqwest::Client;

#[derive(Parser)]
#[clap(about, version, author)]
struct Args {
    /// Specify the OSM network to use for counts. Either an osm.pbf file (which'll produce a .bin
    /// file) or a .bin file from a prior run
    #[clap(long)]
    network: String,

    /// A GeoJSON file with LineString requests
    #[clap(long)]
    requests: String,

    /// How many requests to OSRM to have in-flight at once
    #[clap(long, default_value_t = 10)]
    concurrency: usize,
    /// A percent (0 to 1000 -- note NOT 100) of requests to use
    #[clap(long, default_value_t = 1000)]
    sample_requests: usize,
}

struct Counts {
    count_per_edge: HashMap<(i64, i64), usize>,
    errors: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let mut start = Instant::now();
    println!("Loading network from {}", args.network);
    let network = if args.network.ends_with(".osm.pbf") {
        osm2network::Network::make_from_pbf(args.network)?
    } else {
        osm2network::Network::load_from_bin(args.network)?
    };
    println!("That took {:?}\n", Instant::now().duration_since(start));

    start = Instant::now();
    println!("Loading requests from {}", args.requests);
    let requests = Request::load_from_geojson(&args.requests, args.sample_requests)?;
    println!("That took {:?}\n", Instant::now().duration_since(start));

    let num_requests = requests.len();
    println!(
        "Making {} requests with concurrency = {}",
        HumanCount(num_requests as u64),
        args.concurrency
    );

    start = Instant::now();
    let results = stream::iter(requests)
        .map(|req| tokio::spawn(async { req.calculate_route().await }))
        .buffer_unordered(args.concurrency);

    // Count routes per node pairs
    let progress = ProgressBar::new(num_requests as u64).with_style(ProgressStyle::with_template(
            "[{elapsed_precise}] [{wide_bar:.cyan/blue}] {human_pos}/{human_len} ({per_sec}, {eta})").unwrap());
    let mut counts = Counts {
        count_per_edge: HashMap::new(),
        errors: 0,
    };
    results
        .fold(&mut counts, |accumulate, future| async {
            progress.inc(1);
            // TODO Flatten
            match future {
                Ok(result) => match result {
                    Ok(nodes) => {
                        // OSRM returns all nodes, but we only consider some to be intersections
                        // TODO When the route begins or ends with an intermediate non-intersection
                        // node, we don't handle it well yet
                        let mut i1 = nodes[0];
                        let mut last = nodes[0];
                        for node in nodes.into_iter().skip(1) {
                            if network.intersections.contains(&node) {
                                *accumulate.count_per_edge.entry((i1, node)).or_insert(0) += 1;
                                i1 = node;
                            }
                            last = node;
                        }
                        if i1 != last && false {
                            println!("We didn't end on an intersection... {i1} to {last}");
                        }
                    }
                    Err(err) => {
                        // TODO Usually the API being overloaded
                        if false {
                            println!("Request failed: {err}");
                        }
                        accumulate.errors += 1;
                    }
                },
                Err(err) => {
                    println!("Tokio error: {err}");
                }
            }
            accumulate
        })
        .await;
    progress.finish();

    println!(
        "Got counts for {} edges. That took {:?}",
        HumanCount(counts.count_per_edge.len() as u64),
        Instant::now().duration_since(start)
    );
    println!("There were {} errors\n", HumanCount(counts.errors));

    println!("Writing output GJ");
    start = Instant::now();
    network.write_geojson("output.geojson", counts.count_per_edge)?;
    println!("That took {:?}", Instant::now().duration_since(start));

    Ok(())
}

struct Request {
    x1: f64,
    y1: f64,
    x2: f64,
    y2: f64,
}

impl Request {
    // Returns OSM node IDs
    async fn calculate_route(self) -> Result<Vec<i64>> {
        // TODO How to share, and does it matter?
        let client = Client::new();

        // Alternatively, try bindings (https://crates.io/crates/rsc_osrm)
        let body = client
            .get(format!(
                "http://localhost:5000/route/v1/driving/{},{};{},{}",
                self.x1, self.y1, self.x2, self.y2
            ))
            .query(&[
                ("overview", "false"),
                ("alternatives", "false"),
                ("steps", "false"),
                ("annotations", "nodes"),
            ])
            .send()
            .await?
            .text()
            .await?;
        let json_value: serde_json::Value = serde_json::from_str(&body)?;
        let nodes: Vec<i64> = serde_json::from_value(
            json_value["routes"][0]["legs"][0]["annotation"]["nodes"].clone(),
        )?;
        Ok(nodes)
    }

    fn load_from_geojson(path: &str, sample_requests: usize) -> Result<Vec<Request>> {
        let gj = std::fs::read_to_string(path)?.parse::<GeoJson>()?;
        let mut requests = Vec::new();
        let mut total = 0;
        if let GeoJson::FeatureCollection(collection) = gj {
            for feature in collection.features {
                total += 1;
                // TODO Off by 1
                if total % 1000 > sample_requests {
                    continue;
                }

                if let Some(geometry) = feature.geometry {
                    if let Value::LineString(line_string) = geometry.value {
                        assert_eq!(2, line_string.len());
                        requests.push(Request {
                            x1: line_string[0][0],
                            y1: line_string[0][1],
                            x2: line_string[1][0],
                            y2: line_string[1][1],
                        });
                    }
                }
            }
        }
        Ok(requests)
    }
}

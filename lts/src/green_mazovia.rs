use crate::{parse, Tags, LTS};

// A flow chart would explain this nicely
pub fn green_mazovia(tags: &Tags) -> (LTS, Vec<String>) {
    let mut msgs = Vec::new();

    if is_cycling_forbidden(&tags, &mut msgs) {
        return (LTS::NotAllowed, msgs);
    }

    if let Some(lts) = separate_path(&tags, &mut msgs) {
        return (lts, msgs);
    }

    if is_bike_lane(&tags, &mut msgs) {
        msgs.push(format!("Bike lane is always LTS1"));
        return (LTS::LTS1, msgs);
    }

    if let Some(lts) = non_bicycle_infrastructure(&tags, &mut msgs) {
        return (lts, msgs);
    }

    if let Some(lts) = is_mixed_traffic(&tags, &mut msgs) {
        return (lts, msgs);
    }

    msgs.push("No categories matched".into());
    (LTS::NotAllowed, msgs)
}

fn non_bicycle_infrastructure(tags: &Tags, msgs: &mut Vec<String>) -> Option<LTS> {
    if tags.is("highway", "path") {
        msgs.push(format!(
            "This way is a separated path because highway={}, but not suitable for all bicycles",
            tags.get("highway").unwrap()
        ));
        return Some(LTS::LTS3);
    }

    None
}

fn separate_path(tags: &Tags, msgs: &mut Vec<String>) -> Option<LTS> {
    if tags.is("highway", "cycleway")
        && tags.is("crossing", "traffic_signals") {
        msgs.push(format!(
            "This way is a separated path because highway={}, but have a traffic signal",
            tags.get("highway").unwrap()
        ));
        return Some(LTS::LTS2);
    }

    if tags.is("highway", "cycleway") {
        msgs.push(format!(
            "This way is a separated path because highway={}",
            tags.get("highway").unwrap()
        ));
        return Some(LTS::LTS1);
    }

    if tags.is("highway", "path")
        && tags.is("bicycle", "designated") {
        msgs.push(format!(
            "This way is a separated path because it is path with designated bike cycleway"
        ));
        return Some(LTS::LTS1);
    }

    if tags.is("highway", "path") {
        msgs.push(format!(
            "Ground path can be not suitable for all bicycles"
        ));
        return Some(LTS::LTS4);
    }

    if let Some((key, value)) = tags.prefix_is_any("cycleway", vec!["track", "opposite_track"]) {
        msgs.push(format!(
            "This way is a separated path because {key}={value}"
        ));
        return Some(LTS::LTS1);
    }

    None
}

fn is_bike_lane(tags: &Tags, msgs: &mut Vec<String>) -> bool {
    let mut has_lane = false;
    if let Some((key, value)) = tags.prefix_is_any(
        "cycleway",
        vec![
            "crossing",
            "lane",
            "left",
            "opposite",
            "opposite_lane",
            "right",
            "yes",
        ],
    ) {
        has_lane = true;
        msgs.push(format!("Way has a bike lane because {key}={value}"));
    }

    if tags.is("shoulder:access:bicycle", "yes") {
        msgs.push("Way has a bike lane because shoulder:access:bicycle=yes".into());
        has_lane = true;
    }

    return has_lane;
}

fn is_mixed_traffic(tags: &Tags, msgs: &mut Vec<String>) -> Option<LTS> {
    msgs.push("No bike lane or separated path; treating as mixed traffic".into());

    let speed_limit = parse::get_maxspeed_kmph(tags, msgs);

    if tags.is("motor_vehicle", "no") || tags.is("motorcar", "no") {
        msgs.push("Motor vehicles not allowed, so LTS 1".into());
        return Some(LTS::LTS1);
    }

    if tags.is("highway", "track") {
        msgs.push("LTS 2 because highway=track".into());
        return Some(LTS::LTS2);
    }

    if tags.is("oneway", "yes")
        && tags.is("oneway:bicycle", "no") {
        msgs.push("LTS 2 because it is one way highway with bicycles allowed in both directions".into());
        return Some(LTS::LTS2);
    }

    if speed_limit <= 30 {
        msgs.push("LTS 2 because speed is below 30 kmph".into());
        return Some(LTS::LTS2);
    }

    msgs.push("LTS 4 because speed is over 30 kmph".into());
    Some(LTS::LTS4)
}

fn is_cycling_forbidden(tags: &Tags, msgs: &mut Vec<String>) -> bool {
    if !tags.has("highway") && !tags.has("bicycle") {
        msgs.push("Way doesn't have a highway or bicycle tag".into());
        return true;
    }

    if tags.is("motorroad", "yes") {
        msgs.push("Bicycles are not allowed on motorroads".into());
        return true;
    }

    if tags.is_any("bicycle", vec!["no", "use_sidepath"]) {
        msgs.push("Cycling not permitted due to bicycle=no".into());
        return true;
    }

    if tags.is("access", "no") {
        // TODO There are exceptions for bicycle
        msgs.push("Cycling not permitted due to access=no".into());
        return true;
    }

    if tags.is_any(
        "highway",
        vec!["motorway", "motorway_link", "proposed", "construction"],
    ) {
        msgs.push(format!(
            "Cycling not permitted due to highway={}",
            tags.get("highway").unwrap()
        ));
        return true;
    }

    if let Some((key, value)) = tags.prefix_is_any("cycleway", vec!["separate"]) {
        msgs.push(format!(
            "Cycling not permitted because there is separate cycleway {key}={value}"
        ));
        return true;
    }

    if tags.is_any("highway", vec!["footway"])
        && !tags.is_any("bicycle", vec!["yes", "separated", "designated"])
    {
        msgs.push(format!(
            "Cycling not permitted on highway={}, when footway and bicycle=yes|separated|designated is missing",
            tags.get("highway").unwrap()
        ));
        return true;
    }

    false
}

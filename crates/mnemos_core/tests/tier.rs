use mnemos_core::Tier;

#[test]
fn tier_round_trips_through_string() {
    for tier in [
        Tier::Working,
        Tier::Episodic,
        Tier::Semantic,
        Tier::Procedural,
        Tier::Reflection,
    ] {
        let s = tier.as_str();
        let parsed: Tier = s.parse().unwrap();
        assert_eq!(parsed, tier);
    }
}

#[test]
fn tier_parse_rejects_unknown() {
    assert!("frobnicated".parse::<Tier>().is_err());
}

#[test]
fn tier_directory_names_are_stable() {
    assert_eq!(Tier::Working.dir_name(), "working");
    assert_eq!(Tier::Episodic.dir_name(), "episodic");
    assert_eq!(Tier::Semantic.dir_name(), "semantic");
    assert_eq!(Tier::Procedural.dir_name(), "procedural");
    assert_eq!(Tier::Reflection.dir_name(), "reflections");
}

#[test]
fn tier_serde_uses_kebab_case() {
    let json = serde_json::to_string(&Tier::Working).unwrap();
    assert_eq!(json, "\"working\"");
    let back: Tier = serde_json::from_str(&json).unwrap();
    assert_eq!(back, Tier::Working);
}

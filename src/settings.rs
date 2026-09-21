use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const LAYER_IDS: [&str; 8] = [
    "ai",
    "migrations",
    "patrol",
    "gastroliths",
    "salt",
    "landmarks",
    "water",
    "breadcrumbs",
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct LayerSettings {
    pub visible: bool,
    pub opacity: f64,
    pub icon_size: f64,
    pub max_distance: f64,
}

impl Default for LayerSettings {
    fn default() -> Self {
        Self {
            visible: true,
            opacity: 0.85,
            icon_size: 18.0,
            max_distance: 10_000.0,
        }
    }
}

impl LayerSettings {
    pub fn sanitized(mut self) -> Self {
        self.opacity = self.opacity.clamp(0.0, 1.0);
        self.icon_size = self.icon_size.clamp(6.0, 64.0);
        self.max_distance = self.max_distance.clamp(100.0, 100_000.0);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub name: String,
    pub map_mode: String,
    pub minimap_size: f64,
    pub layers: BTreeMap<String, LayerSettings>,
}

impl Default for Profile {
    fn default() -> Self {
        let layers = LAYER_IDS
            .iter()
            .map(|id| ((*id).to_owned(), LayerSettings::default()))
            .collect();
        Self {
            name: "Default".into(),
            map_mode: "minimap".into(),
            minimap_size: 360.0,
            layers,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorldBounds {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
    pub axis_x_horizontal: bool,
}

pub fn world_to_map(x: f64, y: f64, bounds: WorldBounds) -> Option<(f64, f64)> {
    let width = bounds.max_x - bounds.min_x;
    let height = bounds.max_y - bounds.min_y;
    if width <= 0.0 || height <= 0.0 || !x.is_finite() || !y.is_finite() {
        return None;
    }
    if bounds.axis_x_horizontal {
        Some(((y - bounds.min_y) / height, (x - bounds.min_x) / width))
    } else {
        Some(((x - bounds.min_x) / width, (y - bounds.min_y) / height))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transforms_gateway_axis_swap() {
        let b = WorldBounds {
            min_x: -607_000.0,
            min_y: -505_000.0,
            max_x: 509_000.0,
            max_y: 607_000.0,
            axis_x_horizontal: true,
        };
        let point = world_to_map(-180_095.0, 249_059.0, b).expect("known point");
        assert!((point.0 - 0.6781).abs() < 0.001);
        assert!((point.1 - 0.3825).abs() < 0.001);
        assert_eq!(world_to_map(-607_000.0, -505_000.0, b), Some((0.0, 0.0)));
    }

    #[test]
    fn rejects_uncalibrated_or_invalid_bounds() {
        let b = WorldBounds {
            min_x: 1.0,
            min_y: 0.0,
            max_x: 1.0,
            max_y: 2.0,
            axis_x_horizontal: false,
        };
        assert_eq!(world_to_map(1.0, 1.0, b), None);
        assert_eq!(
            world_to_map(f64::NAN, 1.0, WorldBounds { max_x: 2.0, ..b }),
            None
        );
    }

    #[test]
    fn sanitizes_layer_settings() {
        let settings = LayerSettings {
            visible: false,
            opacity: 4.0,
            icon_size: 2.0,
            max_distance: 1_000_000.0,
        }
        .sanitized();
        assert!(!settings.visible);
        assert_eq!(settings.opacity, 1.0);
        assert_eq!(settings.icon_size, 6.0);
        assert_eq!(settings.max_distance, 100_000.0);
    }

    #[test]
    fn default_profile_has_every_independent_layer() {
        let profile = Profile::default();
        assert!(LAYER_IDS.iter().all(|id| profile.layers.contains_key(*id)));
    }
}

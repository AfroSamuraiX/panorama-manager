//! Pure display-state planning shared by CLI and future GUI frontends.

use crate::adb::is_safe_media_filename;
use crate::config::DisplayState;
use crate::device::{DisplaySettings, ScreenConfig};

/// Valid user-facing metric tokens paired with the exact device labels they map to.
///
/// The cooler renders at most 3. Its catalog also lists CPU Voltage,
/// Motherboard Temperature, and Memory Frequency, but Linux has no
/// vendor-agnostic, unprivileged source for any of them (board-specific Super
/// I/O chips, or root-only `dmidecode`), so they are not offered here.
/// `Date&Time` is driven by the timestamp every sysinfo frame already carries.
const METRIC_TOKENS: &[(&str, &str)] = &[
    ("cpu-temp", "CPU Temperature"),
    ("cpu-usage", "CPU Usage"),
    ("cpu-freq", "CPU Frequency"),
    ("gpu-temp", "GPU Temperature"),
    ("gpu-usage", "GPU Usage"),
    ("gpu-freq", "GPU Frequency"),
    ("gpu-voltage", "GPU Voltage"),
    ("mem-usage", "Memory Utilization"),
    ("datetime", "Date&Time"),
];

#[derive(Debug, thiserror::Error)]
pub enum DisplayPlanError {
    #[error("--media2 / --metrics2 require --split")]
    Pane2RequiresSplit,

    #[error(
        "nothing to do — give a media file, --metrics, --split, --ratio, or an overlay appearance flag"
    )]
    NothingToDo,

    #[error("at most 3 metrics can be shown, got {0}")]
    TooManyMetrics(usize),

    #[error("unknown metric '{token}'. Valid: {valid}")]
    UnknownMetric { token: String, valid: String },

    #[error("duplicate metric '{0}'")]
    DuplicateMetric(String),

    #[error("{0}")]
    InvalidColor(String),

    #[error("invalid device media filename: {0}")]
    InvalidMediaFilename(String),

    #[error(
        "--split needs media in at least one pane — give a filename positional, --media2 <filename>, or have prior split state with media"
    )]
    SplitNeedsMedia,

    #[error(
        "no media on the cooler — upload one with `pctl upload <local-file>` first, then show it with `pctl display <filename>`"
    )]
    FullScreenNeedsMedia,
}

#[derive(Debug, Clone, Default)]
pub struct DisplayPlanInput {
    pub file: Option<String>,
    pub split: bool,
    pub media2: Option<Option<String>>,
    pub metrics: Option<Vec<String>>,
    pub metrics2: Option<Vec<String>>,
    pub badges: Option<Vec<String>>,
    pub badges2: Option<Vec<String>>,
    pub metrics_color: Option<String>,
    pub metrics_align: Option<String>,
    pub metrics_position: Option<String>,
    pub filter: Option<String>,
    pub ratio: Option<String>,
}

#[derive(Debug, Clone)]
pub struct DisplayPlan {
    pub screen: ScreenConfig,
    pub next_state: DisplayState,
    pub pane1_media: Vec<String>,
    pub pane2_media: Vec<String>,
    pub sysinfo_display: Vec<String>,
    pub sysinfo_display2: Vec<String>,
    pub badges: Vec<String>,
    pub badges2: Vec<String>,
    pub screen_ratio: String,
    pub overlay_color: String,
    pub overlay_align: String,
    pub overlay_position: String,
    pub display_filter: String,
    pub appearance_change: bool,
}

/// Build the device screen config and next persisted display state from user
/// input plus the current saved display state.
pub fn plan_display(
    input: DisplayPlanInput,
    mut state: DisplayState,
) -> Result<DisplayPlan, DisplayPlanError> {
    if !input.split && (input.media2.is_some() || input.metrics2.is_some()) {
        return Err(DisplayPlanError::Pane2RequiresSplit);
    }

    let appearance_change = input.metrics_color.is_some()
        || input.metrics_align.is_some()
        || input.metrics_position.is_some()
        || input.filter.is_some();
    let pane2_explicit = input.media2.is_some() || input.metrics2.is_some();
    if input.file.is_none()
        && !input.split
        && !pane2_explicit
        && input.metrics.is_none()
        && !appearance_change
        && input.ratio.is_none()
    {
        return Err(DisplayPlanError::NothingToDo);
    }

    let overlay = match &input.metrics {
        Some(tokens) => Some(resolve_metric_tokens(tokens)?),
        None => None,
    };
    let overlay2 = match &input.metrics2 {
        Some(tokens) => Some(resolve_metric_tokens(tokens)?),
        None => None,
    };
    let color = match &input.metrics_color {
        Some(raw) => Some(parse_overlay_color(raw)?),
        None => None,
    };

    if let Some(filename) = &input.file {
        validate_device_media_filename(filename)?;
    }
    if let Some(Some(filename)) = &input.media2 {
        validate_device_media_filename(filename)?;
    }

    let pane1_media = match &input.file {
        Some(filename) => vec![filename.clone()],
        None => state.media.clone(),
    };

    let pane2_media = if input.split {
        match &input.media2 {
            Some(Some(filename)) => vec![filename.clone()],
            Some(None) => Vec::new(),
            None => state.media2.clone(),
        }
    } else {
        Vec::new()
    };

    if input.split && pane1_media.is_empty() && pane2_media.is_empty() {
        return Err(DisplayPlanError::SplitNeedsMedia);
    }
    if !input.split && input.file.is_none() && pane1_media.is_empty() {
        return Err(DisplayPlanError::FullScreenNeedsMedia);
    }

    let sysinfo_display = overlay.unwrap_or_else(|| state.sysinfo_display.clone());
    let sysinfo_display2 = if input.split {
        overlay2.unwrap_or_else(|| state.sysinfo_display2.clone())
    } else {
        Vec::new()
    };
    let badges = input.badges.unwrap_or_else(|| state.badges.clone());
    let badges2 = if input.split {
        input.badges2.unwrap_or_else(|| state.badges2.clone())
    } else {
        Vec::new()
    };
    let overlay_color = color.unwrap_or_else(|| state.metrics_color.clone());
    let overlay_align = input
        .metrics_align
        .unwrap_or_else(|| state.metrics_align.clone());
    let overlay_position = input
        .metrics_position
        .unwrap_or_else(|| state.metrics_position.clone());
    let display_filter = input.filter.unwrap_or_else(|| state.display_filter.clone());
    let screen_ratio = input
        .ratio
        .unwrap_or_else(|| if input.split { "1:1" } else { "2:1" }.to_string());

    let media_payload = if input.split {
        let pane1 = pane1_media.first().cloned().unwrap_or_default();
        let pane2 = pane2_media.first().cloned().unwrap_or_default();
        vec![pane1, pane2]
    } else {
        pane1_media.clone()
    };

    let settings = DisplaySettings {
        color: overlay_color.clone(),
        align: overlay_align.clone(),
        position: overlay_position.clone(),
        badges: badges.clone(),
        filter_value: display_filter.clone(),
        filter_opacity: display_filter_opacity(&display_filter),
        ..DisplaySettings::default()
    };
    let settings2 = DisplaySettings {
        badges: badges2.clone(),
        ..settings.clone()
    };
    let screen = ScreenConfig {
        media: media_payload,
        screen_mode: if input.split {
            "Screen Splitting".to_string()
        } else {
            "Full Screen".to_string()
        },
        ratio: screen_ratio.clone(),
        sysinfo_display: sysinfo_display.clone(),
        sysinfo_display2: sysinfo_display2.clone(),
        settings: settings.clone(),
        settings2,
        ..ScreenConfig::default()
    };

    state.media = pane1_media.clone();
    state.media2 = pane2_media.clone();
    state.sysinfo_display = sysinfo_display.clone();
    state.sysinfo_display2 = sysinfo_display2.clone();
    state.badges = badges.clone();
    state.badges2 = badges2.clone();
    state.ratio = screen.ratio.clone();
    state.screen_mode = screen.screen_mode.clone();
    state.play_mode = screen.play_mode.clone();
    state.metrics_color = overlay_color.clone();
    state.metrics_align = overlay_align.clone();
    state.metrics_position = overlay_position.clone();
    state.display_filter = display_filter.clone();

    Ok(DisplayPlan {
        screen,
        next_state: state,
        pane1_media,
        pane2_media,
        sysinfo_display,
        sysinfo_display2,
        badges,
        badges2,
        screen_ratio,
        overlay_color,
        overlay_align,
        overlay_position,
        display_filter,
        appearance_change,
    })
}

pub fn display_filter_opacity(filter: &str) -> i32 {
    if filter.is_empty() {
        0
    } else {
        80
    }
}

pub fn validate_device_media_filename(filename: &str) -> Result<(), DisplayPlanError> {
    if !is_safe_media_filename(filename) {
        return Err(DisplayPlanError::InvalidMediaFilename(filename.to_string()));
    }

    Ok(())
}

/// Map metric tokens to the device labels the screen config expects.
/// Errors on an unknown or duplicate token. An empty input yields an empty
/// list so callers can use it to clear the overlay.
pub fn resolve_metric_tokens(tokens: &[String]) -> Result<Vec<String>, DisplayPlanError> {
    if tokens.len() > 3 {
        return Err(DisplayPlanError::TooManyMetrics(tokens.len()));
    }
    let mut labels: Vec<String> = Vec::new();
    for tok in tokens {
        let label = METRIC_TOKENS
            .iter()
            .find(|(t, _)| *t == tok.as_str())
            .map(|(_, label)| (*label).to_string())
            .ok_or_else(|| {
                let valid: Vec<&str> = METRIC_TOKENS.iter().map(|(t, _)| *t).collect();
                DisplayPlanError::UnknownMetric {
                    token: tok.clone(),
                    valid: valid.join(", "),
                }
            })?;
        if labels.contains(&label) {
            return Err(DisplayPlanError::DuplicateMetric(tok.clone()));
        }
        labels.push(label);
    }
    Ok(labels)
}

pub fn metric_label_to_token(label: &str) -> Option<&'static str> {
    METRIC_TOKENS
        .iter()
        .find(|(_, known_label)| *known_label == label)
        .map(|(token, _)| *token)
}

/// Parse a metrics color value into the device's `#RRGGBB` form. Accepts hex
/// (`#RRGGBB` or bare `RRGGBB`) or an `R,G,B` triple (components 0-255).
pub fn parse_overlay_color(input: &str) -> Result<String, DisplayPlanError> {
    let s = input.trim();
    if s.contains(',') {
        let parts: Vec<&str> = s.split(',').map(str::trim).collect();
        if parts.len() != 3 {
            return Err(DisplayPlanError::InvalidColor(format!(
                "color '{s}': an R,G,B triple needs three values, e.g. 255,0,0"
            )));
        }
        let mut rgb = [0u8; 3];
        for (slot, part) in rgb.iter_mut().zip(&parts) {
            *slot = part.parse().map_err(|_| {
                DisplayPlanError::InvalidColor(format!(
                    "color '{s}': R,G,B components must be integers 0-255"
                ))
            })?;
        }
        Ok(format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2]))
    } else {
        let hex = s.strip_prefix('#').unwrap_or(s);
        if hex.len() != 6 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(DisplayPlanError::InvalidColor(format!(
                "color '{s}': expected #RRGGBB hex or an R,G,B triple (e.g. 255,0,0)"
            )));
        }
        Ok(format!("#{}", hex.to_uppercase()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn toks(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn metric_tokens_resolve_to_device_labels() {
        let got = resolve_metric_tokens(&toks(&["cpu-temp", "gpu-temp"])).unwrap();
        assert_eq!(got, vec!["CPU Temperature", "GPU Temperature"]);
    }

    #[test]
    fn metric_tokens_datetime_is_valid() {
        let got = resolve_metric_tokens(&toks(&["datetime"])).unwrap();
        assert_eq!(got, vec!["Date&Time"]);
    }

    #[test]
    fn metric_tokens_empty_input_yields_empty() {
        assert!(resolve_metric_tokens(&[]).unwrap().is_empty());
    }

    #[test]
    fn metric_tokens_reject_unknown() {
        let err = resolve_metric_tokens(&toks(&["cpu-temp", "bogus"])).unwrap_err();
        assert!(err.to_string().contains("unknown metric 'bogus'"));
    }

    #[test]
    fn metric_tokens_reject_duplicates() {
        let err = resolve_metric_tokens(&toks(&["cpu-temp", "cpu-temp"])).unwrap_err();
        assert!(err.to_string().contains("duplicate"));
    }

    #[test]
    fn metric_tokens_reject_more_than_three() {
        let err = resolve_metric_tokens(&toks(&["cpu-temp", "gpu-temp", "cpu-freq", "mem-usage"]))
            .unwrap_err();
        assert!(err.to_string().contains("at most 3"));
    }

    #[test]
    fn overlay_color_parses_hex_and_rgb() {
        assert_eq!(parse_overlay_color("#ff0000").unwrap(), "#FF0000");
        assert_eq!(parse_overlay_color("00ff00").unwrap(), "#00FF00");
        assert_eq!(parse_overlay_color("0,0,255").unwrap(), "#0000FF");
        assert_eq!(parse_overlay_color(" 255, 165, 0 ").unwrap(), "#FFA500");
    }

    #[test]
    fn overlay_color_rejects_bad_input() {
        assert!(parse_overlay_color("#fff").is_err());
        assert!(parse_overlay_color("gggggg").is_err());
        assert!(parse_overlay_color("300,0,0").is_err());
        assert!(parse_overlay_color("1,2").is_err());
    }

    #[test]
    fn plan_display_builds_full_screen_config_and_state() {
        let input = DisplayPlanInput {
            file: Some("clip.mp4".into()),
            metrics: Some(toks(&["cpu-temp"])),
            metrics_color: Some("255,0,0".into()),
            metrics_align: Some("Right".into()),
            metrics_position: Some("Bottom".into()),
            filter: Some("Smoke".into()),
            ..DisplayPlanInput::default()
        };

        let plan = plan_display(input, DisplayState::default()).unwrap();

        assert_eq!(plan.screen.screen_mode, "Full Screen");
        assert_eq!(plan.screen.ratio, "2:1");
        assert_eq!(plan.screen.media, vec!["clip.mp4"]);
        assert_eq!(plan.screen.sysinfo_display, vec!["CPU Temperature"]);
        assert_eq!(plan.screen.settings.color, "#FF0000");
        assert_eq!(plan.screen.settings.align, "Right");
        assert_eq!(plan.screen.settings.position, "Bottom");
        assert_eq!(plan.screen.settings.filter_value, "Smoke");
        assert_eq!(plan.next_state.media, vec!["clip.mp4"]);
        assert_eq!(plan.next_state.display_filter, "Smoke");
    }

    #[test]
    fn plan_display_builds_split_payload_with_dark_second_pane() {
        let input = DisplayPlanInput {
            file: Some("left.mp4".into()),
            split: true,
            media2: Some(None),
            ratio: Some("1:1".into()),
            ..DisplayPlanInput::default()
        };

        let plan = plan_display(input, DisplayState::default()).unwrap();

        assert_eq!(plan.screen.screen_mode, "Screen Splitting");
        assert_eq!(plan.screen.media, vec!["left.mp4", ""]);
        assert!(plan.next_state.media2.is_empty());
    }
}

use async_graphql::SimpleObject;
use chrono::{DateTime, Utc};
use models::xfe_fluorescence_spectrum;

/// Combines autoproc integration, autoproc program, autoproc and autoproc scaling
#[derive(Debug, Clone, SimpleObject)]
#[graphql(name = "Session", complex)]
pub struct Session {
    /// An opaque unique identifier for session
    pub id: u32,
}

/// Represents XFEFluorescenceSpectrum table from the ISPyB database
#[derive(Debug, Clone, SimpleObject)]
#[graphql(name = "FluorescenceScan", unresolvable)]
pub struct FluorescenceScan {
    /// An opaque unique identifier for the XFEFluorescenceSpectrum
    pub id: u32,
    /// An opaque unique identifier for a session
    pub session_id: u32,
    /// Full path of the scan file in jpeg format
    pub jpeg_scan_file_full_path: Option<String>,
    /// Start time of the scan
    pub start_time: Option<DateTime<Utc>>,
    /// End time of the scan
    pub end_time: Option<DateTime<Utc>>,
    /// Scan file name
    pub filename: Option<String>,
    /// Beam exposure time
    pub exposure_time: Option<f32>,
    /// Beam axis position
    pub axis_position: Option<f32>,
    /// Amount of beam transmission
    pub beam_transmission: Option<f32>,
    /// Full path of the scan file
    pub scan_file_full_path: Option<String>,
    /// Amount of energy from the beam
    pub energy: Option<f32>,
    /// Beam verticial size
    pub beam_size_vertical: Option<f32>,
    /// Beam horizontal size
    pub beam_size_horizontal: Option<f32>,
}

impl From<xfe_fluorescence_spectrum::Model> for FluorescenceScan {
    fn from(value: xfe_fluorescence_spectrum::Model) -> Self {
        Self {
            id: value.xfe_fluorescence_spectrum_id,
            session_id: value.session_id,
            jpeg_scan_file_full_path: value.jpeg_scan_file_full_path,
            start_time: value.start_time.map(|time| time.and_utc()),
            end_time: value.end_time.map(|time| time.and_utc()),
            filename: value.filename,
            exposure_time: value.exposure_time,
            axis_position: value.axis_position,
            beam_transmission: value.beam_transmission,
            scan_file_full_path: value.scan_file_full_path,
            energy: value.energy,
            beam_size_vertical: value.beam_size_vertical,
            beam_size_horizontal: value.beam_size_horizontal,
        }
    }
}

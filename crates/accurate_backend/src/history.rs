use std::error::Error;
use std::fmt::{Display, Formatter};

#[derive(Clone, Debug, PartialEq)]
pub struct Su2HistoryValue {
    pub name: String,
    pub value: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Su2HistorySummary {
    pub row_count: usize,
    pub last_iteration: Option<u64>,
    pub residuals: Vec<Su2HistoryValue>,
    pub last_numeric_values: Vec<Su2HistoryValue>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Su2HistoryGateStatus {
    ResidualTargetMet,
    IterationBudgetReached,
    Incomplete,
    NoHistoryRows,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Su2HistoryQuality {
    pub status: Su2HistoryGateStatus,
    pub last_iteration: Option<u64>,
    pub requested_iterations: u32,
    pub residual_target_log10: f64,
    pub max_residual_log10: Option<f64>,
    pub residual_count: usize,
    pub all_residuals_finite: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Su2WorldAxisDiagnostics {
    /// Aggregate force coefficients in SU2/world XYZ coordinates over MARKER_MONITORING.
    pub force_coefficient_xyz: [f64; 3],
    /// Aggregate moment coefficients about the configured reference origin in SU2/world XYZ coordinates.
    pub moment_coefficient_xyz: [f64; 3],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Su2DiagnosticError {
    MissingFields(Vec<String>),
    NonFiniteField(String),
    AmbiguousField(String),
}

impl Display for Su2DiagnosticError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingFields(fields) => write!(
                f,
                "SU2 history is missing aggregate world-axis diagnostic fields: {}",
                fields.join(", ")
            ),
            Self::NonFiniteField(field) => {
                write!(f, "SU2 history diagnostic field {field} is non-finite")
            }
            Self::AmbiguousField(field) => write!(
                f,
                "SU2 history contains multiple aggregate diagnostic fields matching {field}"
            ),
        }
    }
}

impl Error for Su2DiagnosticError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Su2HistoryError {
    Empty,
    UnterminatedQuotedField,
    MissingIterationColumn,
}

impl Display for Su2HistoryError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "SU2 history CSV is empty"),
            Self::UnterminatedQuotedField => write!(f, "SU2 history CSV contains an unterminated quoted field"),
            Self::MissingIterationColumn => write!(f, "SU2 history CSV has no recognized iteration column"),
        }
    }
}

impl Error for Su2HistoryError {}

pub fn summarize_su2_history_csv(text: &str) -> Result<Su2HistorySummary, Su2HistoryError> {
    let mut records = text.lines().filter(|line| !line.trim().is_empty());
    let header_line = records.next().ok_or(Su2HistoryError::Empty)?;
    let headers = parse_csv_record(header_line)?;
    if headers.is_empty() {
        return Err(Su2HistoryError::Empty);
    }

    let normalized = headers
        .iter()
        .map(|header| normalize_header(header))
        .collect::<Vec<_>>();
    let iteration_index = find_iteration_column(&normalized)
        .ok_or(Su2HistoryError::MissingIterationColumn)?;
    let residual_indices = normalized
        .iter()
        .enumerate()
        .filter_map(|(index, header)| header.contains("RMS").then_some(index))
        .collect::<Vec<_>>();

    let mut row_count = 0_usize;
    let mut last_iteration = None;
    let mut last_fields = None::<Vec<String>>;

    for line in records {
        let fields = parse_csv_record(line)?;
        if fields.len() != headers.len() {
            continue;
        }
        let Some(iteration) = parse_iteration(&fields[iteration_index]) else {
            continue;
        };
        row_count += 1;
        last_iteration = Some(iteration);
        last_fields = Some(fields);
    }

    let mut residuals = Vec::new();
    let mut last_numeric_values = Vec::new();
    if let Some(fields) = last_fields {
        for (index, raw) in fields.iter().enumerate() {
            if let Ok(value) = raw.trim().parse::<f64>() {
                last_numeric_values.push(Su2HistoryValue {
                    name: headers[index].clone(),
                    value,
                });
            }
        }
        for index in residual_indices {
            if let Ok(value) = fields[index].trim().parse::<f64>() {
                residuals.push(Su2HistoryValue {
                    name: headers[index].clone(),
                    value,
                });
            }
        }
    }

    Ok(Su2HistorySummary {
        row_count,
        last_iteration,
        residuals,
        last_numeric_values,
    })
}

pub fn evaluate_su2_history_quality(
    summary: &Su2HistorySummary,
    requested_iterations: u32,
    residual_target_log10: f64,
) -> Su2HistoryQuality {
    let all_residuals_finite = !summary.residuals.is_empty()
        && summary.residuals.iter().all(|value| value.value.is_finite());
    let max_residual_log10 = all_residuals_finite.then(|| {
        summary
            .residuals
            .iter()
            .map(|value| value.value)
            .fold(f64::NEG_INFINITY, f64::max)
    });
    let residual_target_met = max_residual_log10
        .is_some_and(|maximum| maximum <= residual_target_log10);
    let iteration_budget_reached = summary
        .last_iteration
        .is_some_and(|iteration| iteration.saturating_add(1) >= u64::from(requested_iterations));

    let status = if summary.row_count == 0 {
        Su2HistoryGateStatus::NoHistoryRows
    } else if !all_residuals_finite {
        Su2HistoryGateStatus::Incomplete
    } else if residual_target_met {
        Su2HistoryGateStatus::ResidualTargetMet
    } else if iteration_budget_reached {
        Su2HistoryGateStatus::IterationBudgetReached
    } else {
        Su2HistoryGateStatus::Incomplete
    };

    Su2HistoryQuality {
        status,
        last_iteration: summary.last_iteration,
        requested_iterations,
        residual_target_log10,
        max_residual_log10,
        residual_count: summary.residuals.len(),
        all_residuals_finite,
    }
}

/// Extracts only the aggregate world-axis coefficient fields from the final usable history row.
/// Per-surface columns such as `CFx(body_42)` intentionally do not match these exact normalized
/// names, so multi-body attribution remains fail-closed until its history semantics are proven.
pub fn extract_su2_world_axis_diagnostics(
    summary: &Su2HistorySummary,
) -> Result<Su2WorldAxisDiagnostics, Su2DiagnosticError> {
    let expected = ["CFX", "CFY", "CFZ", "CMX", "CMY", "CMZ"];
    let mut values = [0.0_f64; 6];
    let mut missing = Vec::new();

    for (slot, field) in expected.iter().enumerate() {
        let matches = summary
            .last_numeric_values
            .iter()
            .filter(|value| normalize_header(&value.name) == *field)
            .collect::<Vec<_>>();
        match matches.as_slice() {
            [] => missing.push((*field).to_owned()),
            [value] if !value.value.is_finite() => {
                return Err(Su2DiagnosticError::NonFiniteField((*field).to_owned()));
            }
            [value] => values[slot] = value.value,
            _ => return Err(Su2DiagnosticError::AmbiguousField((*field).to_owned())),
        }
    }

    if !missing.is_empty() {
        return Err(Su2DiagnosticError::MissingFields(missing));
    }

    Ok(Su2WorldAxisDiagnostics {
        force_coefficient_xyz: [values[0], values[1], values[2]],
        moment_coefficient_xyz: [values[3], values[4], values[5]],
    })
}

fn find_iteration_column(headers: &[String]) -> Option<usize> {
    for candidate in ["INNER_ITER", "OUTER_ITER", "TIME_ITER", "ITER", "ITERATION"] {
        if let Some(index) = headers.iter().position(|header| header == candidate) {
            return Some(index);
        }
    }
    None
}

fn parse_iteration(raw: &str) -> Option<u64> {
    if let Ok(value) = raw.trim().parse::<u64>() {
        return Some(value);
    }
    let value = raw.trim().parse::<f64>().ok()?;
    if !value.is_finite() || value < 0.0 || value.fract() != 0.0 || value > u64::MAX as f64 {
        return None;
    }
    Some(value as u64)
}

fn normalize_header(header: &str) -> String {
    let mut normalized = String::with_capacity(header.len());
    let mut previous_underscore = false;
    for character in header.trim().chars() {
        if character.is_ascii_alphanumeric() {
            normalized.push(character.to_ascii_uppercase());
            previous_underscore = false;
        } else if !previous_underscore {
            normalized.push('_');
            previous_underscore = true;
        }
    }
    normalized.trim_matches('_').to_owned()
}

fn parse_csv_record(line: &str) -> Result<Vec<String>, Su2HistoryError> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();

    while let Some(character) = chars.next() {
        match character {
            '"' if quoted && chars.peek() == Some(&'"') => {
                current.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => {
                fields.push(current.trim().to_owned());
                current.clear();
            }
            _ => current.push(character),
        }
    }
    if quoted {
        return Err(Su2HistoryError::UnterminatedQuotedField);
    }
    fields.push(current.trim().to_owned());
    Ok(fields)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#""Inner_Iter","rms[P]","rms[U]","CL","CD"
0,-2.0,-3.0,0.1,1.2
1,-4.0,-4.5,0.2,1.1
2,-6.5,-6.2,0.3,1.0
"#;

    #[test]
    fn parses_quoted_su2_style_history_and_last_row() {
        let summary = summarize_su2_history_csv(SAMPLE).unwrap();
        assert_eq!(summary.row_count, 3);
        assert_eq!(summary.last_iteration, Some(2));
        assert_eq!(summary.residuals.len(), 2);
        assert_eq!(summary.residuals[0].name, "rms[P]");
        assert_eq!(summary.residuals[0].value, -6.5);
        assert!(summary
            .last_numeric_values
            .iter()
            .any(|value| value.name == "CD" && value.value == 1.0));
    }

    #[test]
    fn residual_gate_is_conservative_across_all_rms_columns() {
        let summary = summarize_su2_history_csv(SAMPLE).unwrap();
        let quality = evaluate_su2_history_quality(&summary, 1000, -6.0);
        assert_eq!(quality.status, Su2HistoryGateStatus::ResidualTargetMet);
        assert_eq!(quality.max_residual_log10, Some(-6.2));
        assert!(quality.all_residuals_finite);
    }

    #[test]
    fn iteration_budget_is_distinct_from_residual_convergence() {
        let text = r#""Inner_Iter","rms[P]"
0,-2.0
1,-3.0
"#;
        let summary = summarize_su2_history_csv(text).unwrap();
        let quality = evaluate_su2_history_quality(&summary, 2, -6.0);
        assert_eq!(quality.status, Su2HistoryGateStatus::IterationBudgetReached);
        assert_eq!(quality.last_iteration, Some(1));
        assert_eq!(quality.max_residual_log10, Some(-3.0));
    }

    #[test]
    fn short_nonconverged_history_is_incomplete() {
        let text = r#""Inner_Iter","rms[P]"
0,-2.0
1,-3.0
"#;
        let summary = summarize_su2_history_csv(text).unwrap();
        let quality = evaluate_su2_history_quality(&summary, 100, -6.0);
        assert_eq!(quality.status, Su2HistoryGateStatus::Incomplete);
    }

    #[test]
    fn nonfinite_residual_does_not_pass_as_iteration_budget_completion() {
        let text = r#""Inner_Iter","rms[P]"
0,-2.0
1,NaN
"#;
        let summary = summarize_su2_history_csv(text).unwrap();
        let quality = evaluate_su2_history_quality(&summary, 2, -6.0);
        assert_eq!(quality.status, Su2HistoryGateStatus::Incomplete);
        assert!(!quality.all_residuals_finite);
        assert_eq!(quality.max_residual_log10, None);
    }

    #[test]
    fn history_without_rms_columns_is_incomplete() {
        let text = r#""Inner_Iter","CD"
0,1.2
1,1.1
"#;
        let summary = summarize_su2_history_csv(text).unwrap();
        let quality = evaluate_su2_history_quality(&summary, 2, -6.0);
        assert_eq!(quality.status, Su2HistoryGateStatus::Incomplete);
        assert_eq!(quality.residual_count, 0);
        assert!(!quality.all_residuals_finite);
    }

    #[test]
    fn aggregate_world_axis_diagnostics_are_extracted_exactly() {
        let summary = summarize_su2_history_csv(
            "\"Inner_Iter\",\"rms[P]\",\"CFx\",\"CFy\",\"CFz\",\"CMx\",\"CMy\",\"CMz\"\n0,-6.5,1.0,2.0,3.0,4.0,5.0,6.0\n",
        )
        .unwrap();
        let diagnostics = extract_su2_world_axis_diagnostics(&summary).unwrap();
        assert_eq!(diagnostics.force_coefficient_xyz, [1.0, 2.0, 3.0]);
        assert_eq!(diagnostics.moment_coefficient_xyz, [4.0, 5.0, 6.0]);
    }

    #[test]
    fn per_surface_columns_do_not_satisfy_aggregate_diagnostics() {
        let summary = summarize_su2_history_csv(
            "\"Inner_Iter\",\"CFx(body_42)\",\"CFy(body_42)\",\"CFz(body_42)\",\"CMx(body_42)\",\"CMy(body_42)\",\"CMz(body_42)\"\n0,1,2,3,4,5,6\n",
        )
        .unwrap();
        assert_eq!(
            extract_su2_world_axis_diagnostics(&summary),
            Err(Su2DiagnosticError::MissingFields(vec![
                "CFX".into(),
                "CFY".into(),
                "CFZ".into(),
                "CMX".into(),
                "CMY".into(),
                "CMZ".into(),
            ]))
        );
    }

    #[test]
    fn nonfinite_world_axis_diagnostic_fails_closed() {
        let summary = summarize_su2_history_csv(
            "\"Inner_Iter\",\"CFx\",\"CFy\",\"CFz\",\"CMx\",\"CMy\",\"CMz\"\n0,1,2,NaN,4,5,6\n",
        )
        .unwrap();
        assert_eq!(
            extract_su2_world_axis_diagnostics(&summary),
            Err(Su2DiagnosticError::NonFiniteField("CFZ".into()))
        );
    }

    #[test]
    fn missing_iteration_column_is_rejected() {
        let error = summarize_su2_history_csv("\"rms[P]\",\"CD\"\n-3,1.0\n").unwrap_err();
        assert_eq!(error, Su2HistoryError::MissingIterationColumn);
    }

    #[test]
    fn doubled_quote_csv_fields_are_supported() {
        let fields = parse_csv_record("\"Inner_Iter\",\"note \"\"quoted\"\"\"\n").unwrap();
        assert_eq!(fields, vec!["Inner_Iter", "note \"quoted\""]);
    }
}

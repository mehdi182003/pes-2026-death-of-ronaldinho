//! A trace of the Direct3D calls of one frame, to see where PES's 3D scene
//! ends and its 2D (HUD, menus) begins.

use std::fmt;

/// One call seen during the traced frame.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    BeginScene,
    EndScene,
    Clear {
        flags: u32,
    },
    SetRenderTarget,
    /// A view matrix, shown by its translation.
    View {
        translation: [f32; 3],
    },
    /// A projection matrix, shown by its X scale (the zoom).
    Projection {
        scale_x: f32,
    },
    World {
        translation: [f32; 3],
        scale: f32,
    },
    /// Vertex format or shader handle given to `SetVertexShader`.
    VertexShader(u32),
    RenderState {
        state: u32,
        value: u32,
    },
    Draw {
        api: &'static str,
        primitives: u32,
    },
    /// Where the mod drew its 3D objects, and why there.
    ModWorld(&'static str),
}

impl fmt::Display for Event {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BeginScene => write!(f, "BeginScene"),
            Self::EndScene => write!(f, "EndScene"),
            Self::Clear { flags } => write!(f, "Clear {flags:#x}"),
            Self::SetRenderTarget => write!(f, "SetRenderTarget"),
            Self::View { translation } => write!(f, "VIEW {translation:?}"),
            Self::Projection { scale_x } => write!(f, "PROJECTION x {scale_x:.3}"),
            Self::World { translation, scale } => {
                write!(f, "WORLD {translation:?} échelle {scale:.3}")
            }
            Self::VertexShader(fvf) => write!(f, "SetVertexShader {fvf:#x}"),
            Self::RenderState { state, value } => write!(f, "SetRenderState {state} = {value:#x}"),
            Self::Draw { api, primitives } => write!(f, "{api} {primitives}"),
            Self::ModWorld(reason) => write!(f, ">>> le mod dessine ses objets 3D ici ({reason})"),
        }
    }
}

/// Lines for the log: runs of draws are merged ("12 × DrawIndexedPrimitiveUP,
/// 3400 primitives"), everything else is listed as is.
pub fn summarize(events: &[Event]) -> Vec<String> {
    let mut lines = Vec::new();
    let mut run: Option<(&str, u32, u64)> = None;
    let flush = |run: &mut Option<(&str, u32, u64)>, lines: &mut Vec<String>| {
        if let Some((api, calls, primitives)) = run.take() {
            lines.push(format!("{calls} × {api}, {primitives} primitives"));
        }
    };
    for event in events {
        match event {
            Event::Draw { api, primitives } => match &mut run {
                Some((run_api, calls, total)) if run_api == api => {
                    *calls += 1;
                    *total += u64::from(*primitives);
                }
                _ => {
                    flush(&mut run, &mut lines);
                    run = Some((api, 1, u64::from(*primitives)));
                }
            },
            other => {
                flush(&mut run, &mut lines);
                lines.push(other.to_string());
            }
        }
    }
    flush(&mut run, &mut lines);
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draws_are_merged_by_run() {
        let events = [
            Event::BeginScene,
            Event::Draw {
                api: "DIP",
                primitives: 10,
            },
            Event::Draw {
                api: "DIP",
                primitives: 5,
            },
            Event::Draw {
                api: "DPUP",
                primitives: 2,
            },
            Event::VertexShader(0x44),
            Event::Draw {
                api: "DPUP",
                primitives: 2,
            },
            Event::EndScene,
        ];
        assert_eq!(
            summarize(&events),
            vec![
                "BeginScene",
                "2 × DIP, 15 primitives",
                "1 × DPUP, 2 primitives",
                "SetVertexShader 0x44",
                "1 × DPUP, 2 primitives",
                "EndScene",
            ]
        );
    }
}

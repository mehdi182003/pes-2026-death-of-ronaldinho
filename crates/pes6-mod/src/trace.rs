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
        /// Vertex format (or shader handle) current at the draw.
        fvf: u32,
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
            Self::Draw {
                api,
                fvf,
                primitives,
            } => write!(f, "{api} (format {fvf:#x}) {primitives}"),
            Self::ModWorld(reason) => write!(f, ">>> le mod dessine ses objets 3D ici ({reason})"),
        }
    }
}

/// Lines for the log: runs of draws with the same call and vertex format are
/// merged ("12 × DrawIndexedPrimitiveUP (format 0x142), 3400 primitives"),
/// everything else is listed as is.
pub fn summarize(events: &[Event]) -> Vec<String> {
    let mut lines = Vec::new();
    let mut run: Option<(&str, u32, u32, u64)> = None;
    let flush = |run: &mut Option<(&str, u32, u32, u64)>, lines: &mut Vec<String>| {
        if let Some((api, fvf, calls, primitives)) = run.take() {
            lines.push(format!(
                "{calls} × {api} (format {fvf:#x}), {primitives} primitives"
            ));
        }
    };
    for event in events {
        match event {
            Event::Draw {
                api,
                fvf,
                primitives,
            } => match &mut run {
                Some((run_api, run_fvf, calls, total)) if run_api == api && run_fvf == fvf => {
                    *calls += 1;
                    *total += u64::from(*primitives);
                }
                _ => {
                    flush(&mut run, &mut lines);
                    run = Some((api, *fvf, 1, u64::from(*primitives)));
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

    fn draw(api: &'static str, fvf: u32, primitives: u32) -> Event {
        Event::Draw {
            api,
            fvf,
            primitives,
        }
    }

    #[test]
    fn draws_are_merged_by_call_and_format() {
        let events = [
            Event::BeginScene,
            draw("DIP", 0x152, 10),
            draw("DIP", 0x152, 5),
            draw("DPUP", 0x152, 2),
            draw("DPUP", 0x144, 2),
            Event::SetRenderTarget,
            draw("DPUP", 0x144, 2),
            Event::EndScene,
        ];
        assert_eq!(
            summarize(&events),
            vec![
                "BeginScene",
                "2 × DIP (format 0x152), 15 primitives",
                "1 × DPUP (format 0x152), 2 primitives",
                "1 × DPUP (format 0x144), 2 primitives",
                "SetRenderTarget",
                "1 × DPUP (format 0x144), 2 primitives",
                "EndScene",
            ]
        );
    }
}

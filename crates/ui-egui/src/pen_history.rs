//! One recent Pen closure linked to document history, plus bounded draft-anchor Redo.
//! Documents remain owned by Session/History; this module never edits document geometry.
use std::sync::Arc;

use photocraft_doc::{Document, LayerId};
use serde_json::{Value, json};

use crate::{PhotocraftApp, state::Tool, vector_ui::PenPath};

#[derive(Clone, PartialEq)]
struct Scope {
    tool: Tool,
    mode: String,
    active: Option<LayerId>,
    selected: Vec<LayerId>,
    path: Option<String>,
    masks: (bool, bool),
    anchors: Vec<[usize; 2]>,
    point_target: Option<crate::direct_select::PathRef>,
    channel: photocraft_engine::channel_cmds::ChannelTarget,
    transient: (bool, bool),
}
impl Scope {
    fn capture(app: &PhotocraftApp) -> Option<Self> {
        let st = app.session.active()?;
        Some(Self {
            tool: app.ui.tool,
            mode: app.ui.tool_options.vector_mode.clone(),
            active: st.active_layer,
            selected: st.selected_layers.clone(),
            path: app.ui.selected_path.clone(),
            masks: (app.ui.mask_target, app.ui.vector_mask_target),
            anchors: app.ui.direct_selection.anchors.clone(),
            point_target: app.ui.direct_selection.target,
            channel: st.channel_view.target,
            transient: (app.ui.transform.is_some(), app.ui.text_edit.is_some()),
        })
    }
}
struct Guard {
    doc: Arc<Document>,
    revision: u64,
    state: u64,
    scope: Scope,
}
impl Guard {
    fn capture(app: &PhotocraftApp) -> Option<Self> {
        let st = app.session.active()?;
        Some(Self { doc: st.doc.clone(), revision: st.revision, state: st.history.row_id(st.history.past_len())?, scope: Scope::capture(app)? })
    }
    fn matches(&self, app: &PhotocraftApp) -> bool {
        app.session.active().is_some_and(|st| {
            Arc::ptr_eq(&st.doc, &self.doc)
                && st.revision == self.revision
                && st.history.row_id(st.history.past_len()) == Some(self.state)
                && Scope::capture(app).as_ref() == Some(&self.scope)
        })
    }
}
pub(crate) struct Pending {
    guard: Guard,
    draft: PenPath,
}
struct Closure {
    guard: Guard,
    before: u64,
    after: u64,
    draft: PenPath,
    path_before: Option<String>,
    path_after: Option<String>,
    opened: bool,
    expected: Option<PenPath>,
    redo: Vec<PenPath>,
}
#[derive(Default)]
pub(crate) struct State {
    pending: Option<Pending>,
    closure: Option<Closure>,
}

/// Clear only our obsolete closure future, never a different document or unrelated redo branch.
pub(crate) fn branch(app: &mut PhotocraftApp) {
    if let Some(c) = app.pen_history.closure.take()
        && c.opened
        && let Some(st) = app.session.active_mut()
        && st.doc.id == c.guard.doc.id
        && st.history.row_id(st.history.past_len()) == Some(c.before)
        && st.history.row_id(st.history.past_len().saturating_add(1)) == Some(c.after)
    {
        st.history.clear_redo();
    }
    app.pen_history.pending = None;
}

/// A changed draft is a new branch, including edits performed by Direct Selection in UI state.
pub(crate) fn reconcile(app: &mut PhotocraftApp) {
    let stale = app.pen_history.closure.as_ref().is_some_and(|c| {
        !c.guard.matches(app)
            || c.expected != app.ui.pen
            || (c.opened && app.ui.pen.is_some() && app.ui.pen_doc != Some(c.guard.doc.id))
            || app.session.active().is_none_or(|st| st.history.row_of(c.before).is_none() || st.history.row_of(c.after).is_none())
    });
    if stale {
        branch(app);
    }
}

/// Capture the open draft before Pen closure changes handles or Curvature closure reflows them.
pub(crate) fn mark_close(app: &mut PhotocraftApp) {
    app.pen_history.pending = match (Guard::capture(app), app.ui.pen.clone()) {
        (Some(guard), Some(draft)) if draft.drawn_with(app.ui.tool) && draft.knots.len() <= 4096 => Some(Pending { guard, draft }),
        _ => None,
    };
}
pub(crate) fn take_close(app: &mut PhotocraftApp, closed: bool) -> Option<Pending> {
    if closed {
        app.pen_history.pending.take()
    } else {
        branch(app);
        None
    }
}
pub(crate) fn committed(app: &mut PhotocraftApp, pending: Option<Pending>) {
    let Some(p) = pending else { return };
    let Some(guard) = Guard::capture(app) else { return };
    let Some(st) = app.session.active() else { return };
    // A refused commit must retain the user's draft, not create a recovery link to an old step.
    if guard.state == p.guard.state && Arc::ptr_eq(&guard.doc, &p.guard.doc) {
        app.ui.pen = Some(p.draft);
        app.ui.pen_doc = Some(guard.doc.id);
        return;
    }
    // Trimmed history and multi-operation commits are not recoverable here.
    if guard.doc.id != p.guard.doc.id || guard.state == p.guard.state || st.history.row_id(st.history.past_len().saturating_sub(1)) != Some(p.guard.state) {
        return;
    }
    app.pen_history.closure = Some(Closure {
        before: p.guard.state,
        after: guard.state,
        path_after: guard.scope.path.clone(),
        guard,
        draft: p.draft,
        path_before: p.guard.scope.path,
        opened: false,
        expected: None,
        redo: Vec::new(),
    });
}

pub(crate) fn can_redo(app: &PhotocraftApp) -> bool {
    app.pen_history.closure.as_ref().is_some_and(|c| c.opened && c.guard.matches(app) && c.expected == app.ui.pen)
}

fn step(app: &mut PhotocraftApp, redo: bool, panel: bool) -> Result<Value, String> {
    if app.session.active_job().is_some() {
        return Err("Pen history cannot move while a document command is running".into());
    }
    let Some(mut c) = app.pen_history.closure.take() else { return Ok(json!(false)) };
    let result = if !redo && !c.opened {
        app.session.execute("edit.undo", json!({})).map_err(|e| e.to_string()).map(|v| {
            if v == json!(true) {
                app.ui.pen = Some(c.draft.clone());
                app.ui.pen_doc = Some(c.guard.doc.id);
                app.ui.selected_path = c.path_before.clone();
                app.ui.direct_selection = Default::default();
                c.opened = true;
            }
            v
        })
    } else if redo && c.opened && !panel && !c.redo.is_empty() {
        app.ui.pen = c.redo.pop();
        app.ui.pen_doc = Some(c.guard.doc.id);
        Ok(json!(true))
    } else if redo && c.opened {
        // Reuse the original engine transaction: retain layer IDs and exact closure handles.
        app.session.execute("edit.redo", json!({})).map_err(|e| e.to_string()).map(|v| {
            if v == json!(true) {
                app.ui.pen = None;
                app.ui.pen_doc = None;
                app.ui.direct_selection = Default::default();
                app.ui.selected_path = c.path_after.clone();
                c.opened = false;
                c.redo.clear();
            }
            v
        })
    } else {
        let was = app.ui.pen.clone();
        if crate::vector_ui::pen_undo_last_point(app) {
            if let Some(was) = was {
                c.redo.push(was);
            }
            Ok(json!(true))
        } else {
            Ok(json!(false))
        }
    };
    c.expected = app.ui.pen.clone();
    if let Some(guard) = Guard::capture(app) {
        c.guard = guard;
    }
    app.pen_history.closure = Some(c);
    app.sync_views();
    result.map(|value| {
        if panel {
            let st = app.session.active();
            json!({"state": st.map(|s| s.history.past_len()), "changed": value == json!(true), "current": st.and_then(|s| s.history.entries().last().cloned())})
        } else {
            value
        }
    })
}

/// All interactive dispatch routes (including History panel rows) meet at App::run_command.
pub(crate) fn command(app: &mut PhotocraftApp, id: &str, params: &Value) -> Option<Result<Value, String>> {
    reconcile(app);
    let undo = matches!(id, "edit.undo" | "history.undo" | "history.stepBackward");
    let redo = matches!(id, "edit.redo" | "history.redo" | "history.stepForward");
    if (undo || redo || id == "history.selectState") && !(params.is_object() || params.is_null()) {
        return Some(Err("history params must be a JSON object".into()));
    }
    if (undo || redo || id == "history.selectState")
        && let Some(authorize) = app.session.authorize
        && let Err(error) = authorize(id, params)
    {
        return Some(Err(error.to_string()));
    }
    if let Some(c) = app.pen_history.closure.as_ref() {
        if undo || redo {
            if undo && c.opened && c.redo.len() >= 64 {
                return Some(Err("Pen draft Undo limit reached; redo or finish the path before continuing".into()));
            }
            if (!undo || !c.opened || app.ui.pen.is_some()) && (!redo || c.opened) {
                let result = step(app, redo, false).map(|value| {
                    if matches!(id, "history.stepBackward" | "history.stepForward") {
                        let st = app.session.active();
                        json!({"states": st.map(|s| s.history.entries().len() + s.history.redo_labels().count()),
                            "current": st.map(|s| s.history.past_len()), "label": st.and_then(|s| s.history.entries().last().cloned())})
                    } else {
                        value
                    }
                });
                return Some(result);
            }
            branch(app);
        } else if id == "history.selectState" {
            // Match history.selectState's integer parameter convention (including UI floats).
            let target = params
                .get("state")
                .and_then(|v| v.as_i64().or_else(|| v.as_f64().filter(|f| f.is_finite()).map(|f| f.round() as i64)))
                .and_then(|v| usize::try_from(v).ok());
            let st = app.session.active()?;
            let row = target.and_then(|i| st.history.row_id(i));
            if row == Some(c.before) && !c.opened {
                return Some(step(app, false, true));
            }
            if row == Some(c.after) && c.opened {
                return Some(step(app, true, true));
            }
            if target.is_some_and(|i| i != st.history.past_len()) && row.is_some() {
                // Only a successful engine jump may discard the interactive draft.
                let opened = c.opened;
                let result = app.session.execute(id, params.clone()).map_err(|e| e.to_string());
                if result.is_ok() {
                    if opened {
                        app.ui.pen = None;
                        app.ui.pen_doc = None;
                        app.ui.direct_selection = Default::default();
                    }
                    // Keep the engine future for this explicit history navigation.
                    app.pen_history = State::default();
                    app.sync_views();
                }
                return Some(result);
            }
        }
    }
    if app.pen_history.closure.as_ref().is_some_and(|c| c.opened)
        && photocraft_engine::commands::find(id).is_some_and(|spec| spec.journal)
        && id != "history.selectState"
    {
        return Some(Err("Finish the reopened Pen path with Enter/Escape before changing the document or its target".into()));
    }
    if undo && crate::vector_ui::pen_undo_last_point(app) {
        return Some(Ok(Value::Null));
    }
    None
}

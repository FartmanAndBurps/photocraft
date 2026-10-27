//! Guarded whole/component translation for Work and saved paths; transient until release.
use photocraft_doc::{LayerContent, LayerId, Path};
use photocraft_geom::Affine;
#[cfg(test)]
use serde_json::json;

use crate::direct_select::PathRef;
use crate::{PhotocraftApp, state::Tool};

#[derive(Clone, Debug, PartialEq)]
struct Selection {
    target: PathRef,
    document: u64,
    revision: u64,
    layer: Option<LayerId>,
    selected_path: Option<String>,
    path: Path,
    component: Option<usize>,
}

#[derive(Clone, Debug, PartialEq)]
struct Gesture {
    start: [f64; 2],
    end: [f64; 2],
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
    gesture: Option<Gesture>,
    selection: Option<Selection>,
    explicit: Option<(u64, Option<LayerId>)>,
    // A cancelled/invalidated gesture must consume its late release, never retarget it.
    owns_release: bool,
    // Release ownership also covers misses; only a captured gesture can be cancelled.
    cancelled: bool,
    // Invalidated component scope must never silently become a whole-path/pixel transform.
    invalid_component: bool,
}

/// Explicit panel selection overrides active Shape/mask priority only in its own context.
pub fn selected_work(app: &PhotocraftApp) -> bool {
    app.ui.selected_path.as_deref() == Some("work")
        && app.session.active().is_some_and(|st| app.ui.work_path_drag.explicit == Some((st.doc.id.0, st.active_layer)) && st.doc.work_path.is_some())
}

pub fn select_whole(app: &mut PhotocraftApp) {
    reset_scope(app);
    app.ui.selected_path = Some("work".into());
    app.ui.work_path_drag.explicit = app.session.active().filter(|st| st.doc.work_path.is_some()).map(|st| (st.doc.id.0, st.active_layer));
    app.ui.direct_selection = Default::default();
}

fn selected_name(app: &PhotocraftApp) -> Option<&str> {
    app.ui.selected_path.as_deref().filter(|name| !matches!(*name, "work" | "layer"))
}
/// Explicit document paths must not be stolen by layer picking or corner widgets.
pub fn explicit_path(app: &PhotocraftApp) -> bool {
    selected_work(app) || selected_name(app).is_some()
}

fn target(app: &PhotocraftApp) -> Option<(PathRef, Path)> {
    if selected_name(app).is_some() {
        return crate::direct_select::active_paths(app).into_iter().next();
    }
    work_target(app).then(|| app.session.active().and_then(|st| st.doc.work_path.clone()).map(|p| (PathRef::Work, p))).flatten()
}

fn work_target(app: &PhotocraftApp) -> bool {
    let Some(st) = app.session.active() else { return false };
    // Preserve upstream layer/mask priority unless a document path is explicitly selected.
    if app.ui.selected_path.as_deref() != Some("work") {
        return false;
    }
    if selected_work(app) {
        return true;
    }
    if let Some(layer) = st.active_layer.and_then(|id| st.doc.layer(id)) {
        if matches!(layer.content, LayerContent::Shape(_)) {
            return false;
        }
        let mask = app.ui.vector_mask_target || st.channel_view.target == photocraft_engine::channel_cmds::ChannelTarget::VectorMask;
        if mask && layer.vector_mask.is_some() {
            return false;
        }
    }
    st.doc.work_path.is_some()
}

fn valid(app: &PhotocraftApp, selection: &Selection) -> bool {
    app.ui.tool == Tool::PathSelection
        && selection.component.is_none_or(|i| selection.path.subpaths.get(i).is_some())
        && app.ui.selected_path == selection.selected_path
        && app.session.active().is_some_and(|st| {
            st.doc.id.0 == selection.document
                && st.revision == selection.revision
                && st.active_layer == selection.layer
                && match &selection.target {
                    PathRef::Work => work_target(app) && st.doc.work_path.as_ref() == Some(&selection.path),
                    PathRef::Named { document, name } => {
                        *document == st.doc.id.0 && st.doc.paths.iter().find(|p| p.name == *name).is_some_and(|p| p.path == selection.path)
                    }
                    _ => false,
                }
        })
}

fn snapshot(app: &PhotocraftApp, target: PathRef, path: Path, component: Option<usize>) -> Option<Selection> {
    let st = app.session.active()?;
    if component.is_some_and(|i| path.subpaths.get(i).is_none()) {
        return None;
    }
    Some(Selection {
        target,
        document: st.doc.id.0,
        revision: st.revision,
        layer: st.active_layer,
        selected_path: app.ui.selected_path.clone(),
        path,
        component,
    })
}

/// Same finite fractional offset for preview and commit, retaining the old click dead zone.
fn displacement(start: [f64; 2], end: [f64; 2]) -> Option<[f64; 2]> {
    let delta = [end[0] - start[0], end[1] - start[1]];
    if !start.iter().chain(end.iter()).all(|v| v.is_finite()) || !delta.iter().all(|v| v.is_finite() && v.abs() <= 1_000_000.0) {
        return None;
    }
    Some(if delta[0].abs() + delta[1].abs() < 0.5 { [0.0, 0.0] } else { delta })
}

/// Called after upstream's existing shape picking, without changing the selected target.
pub fn press(app: &mut PhotocraftApp, at: [f64; 2], component: bool) {
    app.ui.work_path_drag.invalid_component = false;
    app.ui.work_path_drag.cancelled = false;
    app.ui.work_path_drag.gesture = None;
    app.ui.work_path_drag.selection = None;
    app.ui.work_path_drag.owns_release = false;
    // Even missing named targets own the press: never fall back to moving Work.
    app.ui.work_path_drag.owns_release = selected_name(app).is_some();
    let Some((target, path)) = target(app) else {
        return;
    };
    app.ui.work_path_drag.owns_release = true;
    if !app.ui.view.shows(app.ui.view.show.target_path) || displacement(at, at).is_none() {
        return;
    }
    // A missed outline still owns its release and cannot fall through to legacy movement.
    let tol = 6.0 / f64::from(app.point_zoom().max(0.01));
    let hit = photocraft_vector::edit::hit(&path, photocraft_geom::Point::new(at[0], at[1]), tol, &[]);
    let index = match hit {
        Some(photocraft_vector::edit::Hit::Anchor([i, _]) | photocraft_vector::edit::Hit::Segment([i, _], _)) => i,
        _ => return,
    };
    app.ui.work_path_drag.selection = snapshot(app, target, path, component.then_some(index));
    app.ui.work_path_drag.gesture = Some(Gesture { start: at, end: at });
    if component {
        app.ui.status = "Component selected: Option-drag moves it; Cmd+T transforms it; ordinary drag moves the whole path.".into();
        app.ui.status_error = false;
    }
}

/// Canvas supplies document coordinates after its existing constraints, before sample gating.
pub fn motion(app: &mut PhotocraftApp, at: [f64; 2]) {
    let Some(gesture) = app.ui.work_path_drag.gesture.as_ref() else { return };
    if app.ui.work_path_drag.selection.as_ref().is_none_or(|s| !valid(app, s)) || displacement(gesture.start, at).is_none() {
        cancel(app);
        return;
    }
    if let Some(gesture) = &mut app.ui.work_path_drag.gesture {
        gesture.end = at;
    }
}

/// Invalidate immediately after a command, even if a later command restores the target.
pub fn after_command(app: &mut PhotocraftApp, id: &str) {
    if !selected_work(app) || id == "layer.select" || id.starts_with("channel.target") {
        app.ui.work_path_drag.explicit = None;
    }
    if let Some(selection) = &app.ui.work_path_drag.selection
        && (!valid(app, selection) || id == "layer.select" || id.starts_with("channel.target"))
    {
        cancel(app);
    }
}

/// Renderer consumes an offset, not a cloned or serialized path.
pub fn preview(app: &PhotocraftApp) -> Option<[f64; 2]> {
    let gesture = app.ui.work_path_drag.gesture.as_ref()?;
    let selection = app.ui.work_path_drag.selection.as_ref()?;
    let drag = app.drag.as_ref()?;
    if !valid(app, selection) || drag.tool != Tool::PathSelection || drag.start != gesture.start {
        return None;
    }
    displacement(gesture.start, gesture.end)
}

/// Only the captured path receives the live offset; inactive outlines remain fixed.
pub fn preview_for(app: &PhotocraftApp, target: &PathRef, component: usize) -> Option<[f64; 2]> {
    let selection = app.ui.work_path_drag.selection.as_ref()?;
    (selection.target == *target && selection.component.is_none_or(|i| i == component)).then(|| preview(app)).flatten()
}

pub fn selected_component(app: &PhotocraftApp, target: &PathRef) -> Option<usize> {
    let s = app.ui.work_path_drag.selection.as_ref()?;
    (s.target == *target && valid(app, s)).then_some(s.component).flatten()
}

/// Explicit row selection starts whole-path scope; ordinary invalidation does not.
pub fn reset_scope(app: &mut PhotocraftApp) {
    cancel(app);
    app.ui.work_path_drag.invalid_component = false;
}

pub fn component_transform(app: &PhotocraftApp) -> Result<Option<(serde_json::Value, Path)>, String> {
    if app.ui.tool != Tool::PathSelection {
        return Ok(None);
    }
    if app.ui.work_path_drag.invalid_component {
        return Err("Path component selection expired; select its row or component again.".into());
    }
    let Some(s) = app.ui.work_path_drag.selection.as_ref().filter(|s| s.component.is_some()) else { return Ok(None) };
    if !valid(app, s) {
        return Err("Path component target changed; select it again before transforming.".into());
    }
    let Some(index) = s.component else { return Ok(None) };
    let subpath = s.path.subpaths.get(index).ok_or("Path component no longer exists")?;
    let mut params = s.target.params();
    params["subpaths"] = serde_json::json!([index]);
    params["document"] = serde_json::json!(s.document);
    params["revision"] = serde_json::json!(s.revision);
    Ok(Some((params, Path { subpaths: vec![subpath.clone()], fill_rule: s.path.fill_rule, inverted: s.path.inverted })))
}

pub fn transformed(app: &mut PhotocraftApp, params: &serde_json::Value) {
    let index = params.get("subpaths").and_then(|v| v.as_array()).and_then(|a| a.first()).and_then(|v| v.as_u64()).and_then(|v| usize::try_from(v).ok());
    if let (Some(index), Some((target, path))) = (index, target(app))
        && app.session.active().is_some_and(|st| Some(st.doc.id.0) == params.get("document").and_then(|v| v.as_u64()))
        && target.params().get("name") == params.get("name")
    {
        app.ui.work_path_drag.selection = snapshot(app, target, path, Some(index));
        app.ui.work_path_drag.invalid_component = app.ui.work_path_drag.selection.is_none();
    }
}

pub fn owns_drag(app: &PhotocraftApp) -> bool {
    app.ui.work_path_drag.owns_release || app.ui.work_path_drag.selection.as_ref().is_some_and(|s| s.component.is_some() && valid(app, s))
}

pub fn cancel(app: &mut PhotocraftApp) {
    app.ui.work_path_drag.cancelled |= app.ui.work_path_drag.gesture.is_some();
    app.ui.work_path_drag.invalid_component |= app.ui.work_path_drag.selection.as_ref().is_some_and(|s| s.component.is_some());
    app.ui.work_path_drag.gesture = None;
    app.ui.work_path_drag.selection = None;
    if app.drag.as_ref().is_some_and(|d| d.tool == Tool::PathSelection) {
        app.drag = None;
    }
}

pub fn reconcile(app: &mut PhotocraftApp, focused: bool) {
    if !selected_work(app) {
        app.ui.work_path_drag.explicit = None;
    }
    if app.ui.work_path_drag.selection.as_ref().is_some_and(|s| !valid(app, s)) {
        cancel(app);
        return;
    }
    let Some(gesture) = app.ui.work_path_drag.gesture.as_ref() else { return };
    let lost = app.drag.as_ref().is_none_or(|d| d.tool != Tool::PathSelection || d.start != gesture.start);
    if !focused || lost {
        cancel(app);
    }
}

/// None delegates layer/mask gestures to upstream. Owned late releases never delegate.
pub fn finish(app: &mut PhotocraftApp, start: [f64; 2], end: [f64; 2]) -> Option<Result<(), String>> {
    if !std::mem::take(&mut app.ui.work_path_drag.owns_release) {
        return None;
    }
    let cancelled = std::mem::take(&mut app.ui.work_path_drag.cancelled);
    let gesture = app.ui.work_path_drag.gesture.take();
    let selection = app.ui.work_path_drag.selection.take();
    if cancelled {
        return Some(Err("Path drag was cancelled.".into()));
    }
    // A miss or hidden/missing target still consumes release to block legacy Work movement.
    let (Some(gesture), Some(selection)) = (gesture, selection) else { return Some(Ok(())) };
    if start != gesture.start || !valid(app, &selection) {
        return Some(Err("Path drag target changed; no path was moved.".into()));
    };
    let Some([dx, dy]) = displacement(start, end) else { return Some(Err("Invalid path displacement.".into())) };
    if dx == 0.0 && dy == 0.0 {
        if selection.component.is_some() {
            app.ui.work_path_drag.selection = Some(selection);
        }
        return Some(Ok(()));
    }
    let mut params = selection.target.params();
    let result = if let Some(index) = selection.component {
        let anchors = photocraft_vector::edit::subpath_anchors(&selection.path, index);
        params["anchors"] = serde_json::json!(anchors);
        params["move"] = serde_json::json!([dx, dy]);
        app.run("path.moveAnchors", params).map(|_| ())
    } else {
        let moved = selection.path.transform(&Affine::translate(dx, dy));
        params["path"] = photocraft_engine::vector_cmds::path_json(&moved);
        app.run("path.set", params).map(|_| ())
    };
    if result.is_ok()
        && selection.component.is_some()
        && let Some((target, path)) = target(app)
        && target == selection.target
    {
        app.ui.work_path_drag.selection = snapshot(app, target, path, selection.component);
    }
    Some(result)
}

#[cfg(test)]
#[path = "work_path_drag_tests.rs"]
mod tests;

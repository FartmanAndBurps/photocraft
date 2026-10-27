use super::*;
use crate::direct_select::{self, PathRef};

fn app() -> PhotocraftApp {
    let mut a = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    a.session.execute("file.new", json!({"width":300,"height":200})).unwrap();
    a.sync_views();
    for (name, y) in [("work", 20), ("One", 60), ("Two", 100)] {
        a.run(
            "path.set",
            json!({"name":name,"path":{"subpaths":[{"closed":false,"knots":[
                {"anchor":[20,y],"in":[20,y],"out":[20,y]},
                {"anchor":[140,y],"in":[140,y],"out":[140,y]}
            ]}]}}),
        )
        .unwrap();
    }
    a.ui.selected_path = Some("One".into());
    a.ui.tool = Tool::PathSelection;
    a
}
fn shapes(a: &PhotocraftApp) -> Vec<egui::Shape> {
    let ctx = egui::Context::default();
    let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(300.0, 200.0));
        let painter = ui.ctx().layer_painter(egui::LayerId::background()).with_clip_rect(rect);
        let xf = ViewXform { rect, zoom: 1.0, center: [150.0, 100.0], flip: false, rotation: 0.0, aspect: 1.0 };
        draw_overlay(a, &painter, &xf, &a.session.active().unwrap().doc);
    });
    out.textures_delta.clear();
    out.shapes.into_iter().map(|s| s.shape).collect()
}
fn lines(s: &[egui::Shape]) -> Vec<egui::epaint::PathShape> {
    s.iter().filter_map(|s| if let egui::Shape::Path(p) = s { Some(p.clone()) } else { None }).collect()
}
fn anchors(s: &[egui::Shape]) -> usize {
    s.iter().filter(|s| matches!(s, egui::Shape::Rect(_))).count()
}

#[test]
fn show_all_paths_selected_only_and_subdued_references_with_no_extra_anchors() {
    let mut a = app();
    for tool in [Tool::PathSelection, Tool::CurvaturePen, Tool::DirectSelection, Tool::AddAnchorPoint, Tool::Pen, Tool::Brush] {
        a.ui.tool = tool;
        let off = shapes(&a);
        assert_eq!(lines(&off).len(), 1, "{tool:?}: OFF only selected named path");
        a.ui.show_all_paths = true;
        let on = shapes(&a);
        let strokes = lines(&on);
        assert_eq!(strokes.len(), 3, "{tool:?}");
        assert_eq!(anchors(&on), anchors(&off), "references never add anchors");
        let selected = lines(&off).remove(0);
        assert_eq!(strokes.iter().filter(|p| p.points == selected.points).count(), 1);
        assert!(strokes.iter().filter(|p| p.points != selected.points).all(|p| p.stroke != selected.stroke));
        a.ui.show_all_paths = false;
    }
}
#[test]
fn show_all_paths_deselected_work_is_neither_shown_nor_a_hit_target() {
    let mut a = app();
    a.ui.selected_path = None;
    for enabled in [false, true] {
        a.ui.show_all_paths = enabled;
        assert_eq!(lines(&shapes(&a)).len(), if enabled { 3 } else { 0 });
        assert!(!direct_select::candidates(&a).iter().any(|(t, _)| matches!(t, PathRef::Work | PathRef::Named { .. })));
        let before = a.session.active().unwrap().doc.work_path.clone();
        crate::work_path_drag::press(&mut a, [80.0, 20.0], false);
        path_selection_finish(&mut a, [80.0, 20.0], [100.0, 30.0]);
        assert_eq!(a.session.active().unwrap().doc.work_path, before);
    }
}
#[test]
fn show_all_paths_toggle_does_not_change_targets_history_or_drawing_destination() {
    let mut a = app();
    let before = a.session.active().unwrap().doc.clone();
    let history = a.session.active().unwrap().history.past_len();
    let targets = direct_select::candidates(&a);
    for enabled in [true, false, true] {
        a.ui.show_all_paths = enabled;
        assert_eq!(direct_select::candidates(&a), targets);
        assert_eq!(a.session.active().unwrap().history.past_len(), history);
        assert_eq!(a.session.active().unwrap().doc.work_path, before.work_path);
        assert_eq!(a.session.active().unwrap().doc.paths, before.paths);
        assert_eq!(a.ui.selected_path.as_deref(), Some("One"));
    }
    a.ui.tool_options.vector_mode = "path".into();
    a.ui.pen = Some(PenPath { knots: vec![[[30.0, 140.0]; 3], [[100.0, 140.0]; 3]], ..Default::default() });
    pen_commit(&mut a, false);
    assert_eq!(a.session.active().unwrap().doc.paths[0].path.subpaths.len(), 2);
    assert_eq!(a.session.active().unwrap().doc.work_path, before.work_path);
}

#[test]
fn show_all_paths_checkbox_roundtrips_and_survives_panel_and_document_switches() {
    use egui_kittest::kittest::Queryable;
    let a = app();
    let history = a.session.active().unwrap().history.past_len();
    let owner = a.session.active().unwrap().doc.id;
    let mut h = egui_kittest::Harness::builder().with_size(vec2(320.0, 400.0)).build_ui_state(|ui, a: &mut PhotocraftApp| paths_panel(a, ui), a);
    h.run_steps(2);
    h.get_by_label("Show All Paths").click();
    h.run_steps(2);
    assert!(h.state().ui.show_all_paths);
    assert_eq!(h.state().ui.selected_path.as_deref(), Some("One"));
    assert_eq!(h.state().session.active().unwrap().history.past_len(), history);
    // The preference belongs to the UI, not one document or a panel instance.
    h.state_mut().run("file.new", json!({"width":200,"height":200})).unwrap();
    h.run_steps(2);
    assert!(h.state().ui.show_all_paths);
    assert_eq!(lines(&shapes(h.state())).len(), 0, "never shows the other document's saved paths");
    assert!(h.state_mut().session.set_active(0));
    assert_eq!(h.state().session.active().unwrap().doc.id, owner);
    h.state_mut().sync_views();
    h.run_steps(2);
    assert!(h.state().ui.show_all_paths);
    assert_eq!(lines(&shapes(h.state())).len(), 3);
    let saved = serde_json::to_value(&h.state().ui).unwrap();
    let restored: crate::state::UiState = serde_json::from_value(saved.clone()).unwrap();
    assert!(restored.show_all_paths);
    let mut old = saved;
    old.as_object_mut().unwrap().remove("show_all_paths");
    let restored: crate::state::UiState = serde_json::from_value(old).unwrap();
    assert!(!restored.show_all_paths);
    h.get_by_label("Show All Paths").click();
    h.run_steps(2);
    assert!(!h.state().ui.show_all_paths);
}

#[test]
fn show_all_paths_live_updates_rename_delete_edit_select_and_deduplicates() {
    let mut a = app();
    a.ui.show_all_paths = true;
    let work = a.session.active().unwrap().doc.work_path.clone().unwrap();
    a.run("path.set", json!({"name":"Copy","path":photocraft_engine::vector_cmds::path_json(&work)})).unwrap();
    assert_eq!(lines(&shapes(&a)).len(), 3, "identical reference outlines drawn only once");
    a.run("path.rename", json!({"name":"Two","to":"Renamed"})).unwrap();
    assert_eq!(lines(&shapes(&a)).len(), 3);
    a.ui.selected_path = Some("Renamed".into());
    let before = lines(&shapes(&a));
    a.run("path.moveAnchors", json!({"name":"Renamed","anchors":[[0,0]],"move":[10.0,5.0]})).unwrap();
    assert_ne!(lines(&shapes(&a)), before);
    a.run("edit.undo", json!({})).unwrap();
    assert_eq!(lines(&shapes(&a)), before);
    a.run("edit.redo", json!({})).unwrap();
    assert_ne!(lines(&shapes(&a)), before);
    a.run("path.delete", json!({"name":"Copy"})).unwrap();
    a.run("path.delete", json!({"name":"Renamed"})).unwrap();
    assert_eq!(lines(&shapes(&a)).len(), 2);
    a.ui.show_all_paths = false;
    assert_eq!(lines(&shapes(&a)).len(), 0, "missing selection never falls back to Work");
    a.ui.selected_path = Some("work".into());
    assert_eq!(lines(&shapes(&a)).len(), 1);
}

#[test]
fn show_all_paths_references_never_become_hits_and_selected_drag_still_previews() {
    use crate::canvas::{ToolEvent, tool_event};
    let mut a = app();
    a.ui.show_all_paths = true;
    let original = a.session.active().unwrap().doc.paths.clone();
    let work = a.session.active().unwrap().doc.work_path.clone();
    for tool in [Tool::CurvaturePen, Tool::DirectSelection, Tool::AddAnchorPoint, Tool::PathSelection] {
        a.ui.tool = tool;
        let candidates = direct_select::candidates(&a);
        assert_eq!(candidates.len(), 1, "{tool:?}");
        assert!(matches!(&candidates[0].0,PathRef::Named{name,..} if name=="One"));
    }
    for component in [false, true] {
        a.ui.tool = Tool::PathSelection;
        crate::work_path_drag::press(&mut a, [80.0, 100.0], component);
        path_selection_finish(&mut a, [80.0, 100.0], [90.0, 110.0]);
        assert_eq!(a.session.active().unwrap().doc.paths, original);
    }
    a.ui.tool = Tool::AddAnchorPoint;
    tool_event(&mut a, ToolEvent::Down { x: 80.0, y: 100.0, pressure: 1.0 }, egui::Modifiers::NONE);
    tool_event(&mut a, ToolEvent::Up { x: 80.0, y: 100.0 }, egui::Modifiers::NONE);
    assert_eq!(a.session.active().unwrap().doc.paths, original);
    a.ui.tool = Tool::PathSelection;
    crate::work_path_drag::press(&mut a, [80.0, 60.0], true);
    crate::work_path_drag::motion(&mut a, [90.0, 70.0]);
    let during = shapes(&a);
    assert_eq!(lines(&during).len(), 3);
    assert_eq!(a.session.active().unwrap().doc.paths, original);
    path_selection_finish(&mut a, [80.0, 60.0], [90.0, 70.0]);
    assert_eq!(a.session.active().unwrap().doc.paths[1], original[1]);
    assert_eq!(a.session.active().unwrap().doc.work_path, work);
}

#[test]
fn show_all_paths_honours_target_path_visibility_and_optional_metal_snapshot() {
    let mut a = app();
    a.ui.show_all_paths = true;
    a.ui.view.show.target_path = false;
    assert_eq!(lines(&shapes(&a)).len(), 0);
    a.ui.view.show.target_path = true;
    if let Some(dir) = std::env::var_os("PHOTOCRAFT_SHOW_PATHS_SNAPSHOT_DIR") {
        let mut h = egui_kittest::Harness::builder().with_size(vec2(600.0, 400.0)).build_ui_state(
            |ui, a: &mut PhotocraftApp| {
                egui::Panel::left("paths").exact_size(230.0).show(ui, |ui| paths_panel(a, ui));
                egui::CentralPanel::default().show(ui, |ui| {
                    let rect = ui.max_rect();
                    let xf = ViewXform { rect, zoom: 1.0, center: [150.0, 100.0], flip: false, rotation: 0.0, aspect: 1.0 };
                    draw_overlay(a, ui.painter(), &xf, &a.session.active().unwrap().doc);
                });
            },
            a,
        );
        PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ALL[0]);
        h.run_steps(3);
        h.render().unwrap().save(std::path::PathBuf::from(dir).join("show-all-paths.png")).unwrap();
    }
}

#[test]
fn show_all_paths_selected_work_is_exclusive_and_toggle_during_drag_preserves_scope() {
    let mut a = app();
    crate::work_path_drag::select_whole(&mut a);
    assert_eq!(direct_select::candidates(&a).iter().map(|(t, _)| t).collect::<Vec<_>>(), vec![&PathRef::Work]);
    let before = shapes(&a);
    assert_eq!(lines(&before).len(), 1);
    let saved = a.session.active().unwrap().doc.paths.clone();
    let history = a.session.active().unwrap().history.past_len();
    crate::work_path_drag::press(&mut a, [80.0, 20.0], true);
    crate::work_path_drag::motion(&mut a, [90.0, 30.0]);
    a.ui.show_all_paths = true;
    let on = shapes(&a);
    assert_eq!(lines(&on).len(), 3);
    assert_eq!(anchors(&on), anchors(&before));
    assert_eq!(a.session.active().unwrap().history.past_len(), history);
    path_selection_finish(&mut a, [80.0, 20.0], [90.0, 30.0]);
    assert_eq!(a.session.active().unwrap().history.past_len(), history + 1);
    assert_eq!(a.session.active().unwrap().doc.paths, saved);
}

#[test]
fn show_all_paths_clearing_selection_cancels_stale_direct_work_drag() {
    use crate::canvas::{ToolEvent, tool_event};
    let mut a = app();
    a.ui.selected_path = Some("work".into());
    a.ui.tool = Tool::DirectSelection;
    let before = a.session.active().unwrap().doc.work_path.clone();
    let history = a.session.active().unwrap().history.past_len();
    tool_event(&mut a, ToolEvent::Down { x: 20.0, y: 20.0, pressure: 1.0 }, egui::Modifiers::NONE);
    a.ui.selected_path = None;
    a.ui.show_all_paths = true;
    tool_event(&mut a, ToolEvent::Up { x: 40.0, y: 40.0 }, egui::Modifiers::NONE);
    assert_eq!(a.session.active().unwrap().doc.work_path, before);
    assert_eq!(a.session.active().unwrap().history.past_len(), history);
}

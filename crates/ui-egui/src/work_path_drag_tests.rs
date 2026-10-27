use super::*;
use crate::canvas::{ToolEvent, tool_event};
use photocraft_doc::vector::Subpath;

fn app() -> PhotocraftApp {
    let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    app.run("file.new", json!({"width": 400, "height": 200, "background": "white"})).unwrap();
    let mut path = photocraft_vector::shapes::rect(10.0, 20.0, 50.0, 50.0);
    path.subpaths.extend(photocraft_vector::shapes::ellipse(100.0, 20.0, 80.0, 60.0).subpaths);
    path.subpaths.push(Subpath::polyline(&[(220.0, 40.0), (300.0, 60.0)]));
    app.run("path.set", json!({"path": photocraft_engine::vector_cmds::path_json(&path)})).unwrap();
    app.ui.selected_path = Some("work".into());
    app.ui.tool = Tool::PathSelection;
    app.ui.view.extras = true;
    app.ui.view.show.target_path = true;
    app.sync_views();
    app
}
fn work(app: &PhotocraftApp) -> Path {
    app.session.active().unwrap().doc.work_path.clone().unwrap()
}
fn event(app: &mut PhotocraftApp, event: ToolEvent) {
    tool_event(app, event, egui::Modifiers::NONE);
}
fn down(app: &mut PhotocraftApp, at: [f64; 2]) {
    event(app, ToolEvent::Down { x: at[0], y: at[1], pressure: 1.0 });
}
fn moving(app: &mut PhotocraftApp, at: [f64; 2]) {
    event(app, ToolEvent::Move { x: at[0], y: at[1], pressure: 1.0 });
}
fn up(app: &mut PhotocraftApp, at: [f64; 2]) {
    event(app, ToolEvent::Up { x: at[0], y: at[1] });
}

#[test]
fn whole_path_previews_before_release_without_mutation_then_commits_once() {
    let mut app = app();
    let before = work(&app);
    let history = app.session.active().unwrap().history.past_len();
    let revision = app.session.active().unwrap().revision;
    let pixels = photocraft_compose::flatten(&app.session.active().unwrap().doc);
    down(&mut app, [10.0, 20.0]);
    for delta in [[1.125, 2.25], [15.375, -3.125], [-5.25, 8.125]] {
        moving(&mut app, [10.0 + delta[0], 20.0 + delta[1]]);
        assert_eq!(preview(&app), Some(delta));
        assert_eq!(work(&app), before);
        assert_eq!(app.session.active().unwrap().history.past_len(), history);
        assert_eq!(app.session.active().unwrap().revision, revision);
    }
    up(&mut app, [4.75, 28.125]);
    let after = before.transform(&Affine::translate(-5.25, 8.125));
    assert_eq!(work(&app), after);
    assert!(preview(&app).is_none());
    assert_eq!(app.session.active().unwrap().history.past_len(), history + 1);
    assert_eq!(photocraft_compose::flatten(&app.session.active().unwrap().doc), pixels);
    app.run("edit.undo", json!({})).unwrap();
    assert_eq!(work(&app), before);
    app.run("edit.redo", json!({})).unwrap();
    assert_eq!(work(&app), after);
    down(&mut app, [4.75, 28.125]);
    moving(&mut app, [8.0, 30.0]);
    let layer = app.session.active().unwrap().active_layer.unwrap();
    app.run("layer.select", json!({"layer": layer.0})).unwrap();
    assert!(preview(&app).is_none());
    assert!(finish(&mut app, [4.75, 28.125], [8.0, 30.0]).unwrap().is_err());
    assert_eq!(work(&app), after);
}

#[test]
fn endpoint_updates_below_generic_stroke_sampling_threshold() {
    let mut app = app();
    down(&mut app, [10.0, 20.0]);
    moving(&mut app, [11.0, 20.0]);
    let samples = app.drag.as_ref().unwrap().points.len();
    moving(&mut app, [11.0625, 20.03125]);
    assert_eq!(app.drag.as_ref().unwrap().points.len(), samples);
    assert_eq!(preview(&app), Some([1.0625, 0.03125]));
    up(&mut app, [11.0625, 20.03125]);
    assert_eq!(work(&app).subpaths[0].knots[0].anchor, photocraft_geom::Point::new(11.0625, 20.03125));
}

#[test]
fn click_dead_zone_and_invalid_displacement_do_not_create_history() {
    let mut app = app();
    let before = work(&app);
    let history = app.session.active().unwrap().history.past_len();
    down(&mut app, [10.0, 20.0]);
    moving(&mut app, [10.125, 20.125]);
    assert_eq!(preview(&app), Some([0.0, 0.0]));
    up(&mut app, [10.125, 20.125]);
    assert_eq!(work(&app), before);
    assert_eq!(app.session.active().unwrap().history.past_len(), history);
    assert!(displacement([0.0, 0.0], [f64::NAN, 0.0]).is_none());
    assert!(displacement([0.0, 0.0], [f64::INFINITY, 0.0]).is_none());
    assert!(displacement([0.0, 0.0], [1_000_001.0, 0.0]).is_none());
}

#[test]
fn escape_uses_real_shortcut_and_rejects_late_release() {
    let mut app = app();
    let before = work(&app);
    let history = app.session.active().unwrap().history.past_len();
    down(&mut app, [10.0, 20.0]);
    moving(&mut app, [20.125, 25.5]);
    let ctx = egui::Context::default();
    PhotocraftApp::setup_context(&ctx, crate::theme::ThemeKind::ALL[0]);
    let mut input = egui::RawInput::default();
    input.events.push(egui::Event::Key { key: egui::Key::Escape, physical_key: None, pressed: true, repeat: false, modifiers: egui::Modifiers::NONE });
    ctx.run_ui(input, |ui| crate::shortcuts::handle(&mut app, ui.ctx())).textures_delta.clear();
    assert!(app.drag.is_none());
    assert!(preview(&app).is_none());
    assert!(finish(&mut app, [10.0, 20.0], [20.125, 25.5]).unwrap().is_err());
    assert_eq!(work(&app), before);
    assert_eq!(app.session.active().unwrap().history.past_len(), history);
}

#[test]
fn stale_document_revision_geometry_target_and_tool_never_fall_back() {
    for reason in ["revision", "geometry", "document", "target", "tool", "lost", "focus"] {
        let mut app = app();
        down(&mut app, [10.0, 20.0]);
        moving(&mut app, [20.0, 25.0]);
        match reason {
            "revision" => app.session.active_mut().unwrap().revision += 1,
            "geometry" => {
                std::sync::Arc::make_mut(&mut app.session.active_mut().unwrap().doc).work_path = Some(photocraft_vector::shapes::rect(1.0, 1.0, 10.0, 10.0))
            }
            "document" => {
                app.run("file.new", json!({"width": 100, "height": 100})).unwrap();
            }
            "target" => app.ui.selected_path = Some("Saved".into()),
            "tool" => app.ui.tool = Tool::DirectSelection,
            "lost" => app.drag = None,
            "focus" => reconcile(&mut app, false),
            _ => assert!(matches!(reason, "revision" | "geometry" | "document" | "target" | "tool" | "lost" | "focus")),
        }
        reconcile(&mut app, true);
        let before = app.session.active().unwrap().doc.work_path.clone();
        let history = app.session.active().unwrap().history.past_len();
        assert!(preview(&app).is_none(), "{reason}");
        assert!(finish(&mut app, [10.0, 20.0], [20.0, 25.0]).unwrap().is_err(), "{reason}");
        assert_eq!(app.session.active().unwrap().doc.work_path, before, "{reason}");
        assert_eq!(app.session.active().unwrap().history.past_len(), history, "{reason}");
    }
}

#[test]
fn named_misses_are_owned_while_shape_layers_and_masks_keep_their_route() {
    let mut app = app();
    app.run("path.set", json!({"name": "Saved", "path": {"subpaths": [{"knots": [[5,5],[15,15]]}]}})).unwrap();
    app.ui.selected_path = Some("Saved".into());
    let before = app.session.active().unwrap().doc.clone();
    press(&mut app, [10.0, 20.0], false);
    assert!(preview(&app).is_none());
    assert!(finish(&mut app, [10.0, 20.0], [20.0, 20.0]).unwrap().is_ok());
    assert_eq!(app.ui.selected_path.as_deref(), Some("Saved"));
    assert_eq!(app.session.active().unwrap().doc, before);
    app.ui.selected_path = Some("work".into());
    app.run("layer.vectorMask.revealAll", json!({})).unwrap();
    press(&mut app, [10.0, 20.0], false);
    assert!(finish(&mut app, [10.0, 20.0], [20.0, 20.0]).is_none());
    app.ui.vector_mask_target = false;
    app.run("shape.create", json!({"kind":"rect", "rect":[10,20,40,40]})).unwrap();
    press(&mut app, [10.0, 20.0], false);
    assert!(finish(&mut app, [10.0, 20.0], [20.0, 20.0]).is_none());
}

fn overlay(app: &PhotocraftApp, xf: crate::canvas::ViewXform) -> Vec<egui::Shape> {
    let ctx = egui::Context::default();
    PhotocraftApp::setup_context(&ctx, crate::theme::ThemeKind::ALL[0]);
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        let painter = ui.ctx().layer_painter(egui::LayerId::background());
        crate::vector_ui::draw_overlay(app, &painter, &xf, &app.session.active().unwrap().doc);
    });
    output.textures_delta.clear();
    output.shapes.into_iter().map(|s| s.shape).collect()
}

#[test]
fn actual_overlay_moves_all_outlines_and_anchors_once() {
    let mut app = app();
    down(&mut app, [10.0, 20.0]);
    let xf = crate::canvas::ViewXform {
        rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0)),
        zoom: 1.0,
        center: [0.0, 0.0],
        flip: false,
        rotation: 0.0,
        aspect: 1.0,
    };
    let before = overlay(&app, xf);
    moving(&mut app, [21.25, 24.5]);
    let during = overlay(&app, xf);
    let paths = |shapes: &[egui::Shape]| {
        shapes
            .iter()
            .filter_map(|s| match s {
                egui::Shape::Path(p) => Some(p.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let a = paths(&before);
    let b = paths(&during);
    assert_eq!(a.len(), 3);
    assert_eq!(b.len(), 3, "no duplicate original outlines");
    for (a, b) in a.iter().zip(&b) {
        assert_eq!(a.points.len(), b.points.len());
        for (a, b) in a.points.iter().zip(&b.points) {
            assert!((*b - *a - egui::vec2(11.25, 4.5)).length() < 0.001);
        }
    }
    let anchors = |shapes: &[egui::Shape]| {
        shapes
            .iter()
            .filter_map(|s| match s {
                egui::Shape::Rect(r) => Some(r.rect.center()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let a = anchors(&before);
    let b = anchors(&during);
    assert_eq!(a.len(), 20);
    assert_eq!(b.len(), 20);
    for (a, b) in a.iter().zip(&b) {
        assert!((*b - *a - egui::vec2(11.25, 4.5)).length() < 0.001);
    }

    // Optional visual artifact from this same tested overlay, without launching an app.
    if let Some(dir) = std::env::var_os("PHOTOCRAFT_WORK_PATH_SNAPSHOT_DIR") {
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(800.0, 600.0)).build_ui_state(
            move |ui, app: &mut PhotocraftApp| {
                egui::CentralPanel::default().show(ui, |ui| {
                    crate::vector_ui::draw_overlay(app, ui.painter(), &xf, &app.session.active().unwrap().doc);
                });
            },
            app,
        );
        PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ALL[0]);
        h.run_steps(3);
        h.render().unwrap().save(std::path::PathBuf::from(dir).join("live-work-path.png")).unwrap();
    }
}

#[test]
fn view_mapping_fractional_offsets_and_transient_state_are_preserved() {
    for (zoom, rotation, flip, aspect) in [(0.25, 0.0, false, 1.0), (2.0, 30.0, false, 1.0), (8.0, 90.0, true, 1.5)] {
        let mut app = app();
        let xf = crate::canvas::ViewXform {
            rect: egui::Rect::from_min_size(egui::pos2(10.0, 20.0), egui::vec2(800.0, 600.0)),
            zoom,
            center: [80.0, 60.0],
            flip,
            rotation,
            aspect,
        };
        let start = xf.to_doc(xf.to_screen(10.0, 20.0));
        let end = xf.to_doc(xf.to_screen(21.25, 24.5));
        app.ui.extras.snap = true;
        app.ui.view.snap_to.grid = true;
        let before = work(&app);
        down(&mut app, start);
        moving(&mut app, end);
        let delta = preview(&app).unwrap();
        assert!((delta[0] - (end[0] - start[0])).abs() < 1e-8);
        assert!((delta[1] - (end[1] - start[1])).abs() < 1e-8);
        assert!(serde_json::to_value(&app.ui).unwrap().get("work_path_drag").is_none());
        up(&mut app, end);
        assert_eq!(work(&app), before.transform(&Affine::translate(delta[0], delta[1])));
    }
}

fn canvas_frame(app: &mut PhotocraftApp, primary: bool, focused: bool) {
    let ctx = egui::Context::default();
    PhotocraftApp::setup_context(&ctx, crate::theme::ThemeKind::ALL[0]);
    let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
    let input = egui::RawInput { screen_rect: Some(rect), focused, ..Default::default() };
    let view = app.ui.views[0].clone();
    ctx.run_ui(input, |ui| {
        crate::canvas::canvas_view(app, ui, 0, rect, view.clone(), primary);
    })
    .textures_delta
    .clear();
}

#[test]
fn secondary_canvas_focus_does_not_cancel_primary_drag_or_split_history() {
    let mut app = app();
    let before = work(&app);
    let history = app.session.active().unwrap().history.past_len();
    down(&mut app, [10.0, 20.0]);
    moving(&mut app, [21.25, 24.5]);
    assert_eq!(preview(&app), Some([11.25, 4.5]));
    canvas_frame(&mut app, true, true);
    assert_eq!(preview(&app), Some([11.25, 4.5]));
    canvas_frame(&mut app, false, false);
    assert_eq!(preview(&app), Some([11.25, 4.5]), "secondary view must not cancel the primary drag");
    assert!(app.drag.is_some());
    assert_eq!(work(&app), before);
    assert_eq!(app.session.active().unwrap().history.past_len(), history);
    moving(&mut app, [23.375, 26.125]);
    canvas_frame(&mut app, false, false);
    assert_eq!(preview(&app), Some([13.375, 6.125]));
    up(&mut app, [23.375, 26.125]);
    let after = before.transform(&Affine::translate(13.375, 6.125));
    assert_eq!(work(&app), after);
    assert_eq!(app.session.active().unwrap().history.past_len(), history + 1);
    app.run("edit.undo", json!({})).unwrap();
    assert_eq!(work(&app), before);
    app.run("edit.redo", json!({})).unwrap();
    assert_eq!(work(&app), after);
}

#[test]
fn primary_canvas_focus_loss_cancels_drag_without_history_or_late_release() {
    let mut app = app();
    let before = work(&app);
    let history = app.session.active().unwrap().history.past_len();
    down(&mut app, [10.0, 20.0]);
    moving(&mut app, [21.25, 24.5]);
    assert!(preview(&app).is_some());
    canvas_frame(&mut app, true, false);
    assert!(preview(&app).is_none());
    assert!(app.drag.is_none());
    assert!(finish(&mut app, [10.0, 20.0], [21.25, 24.5]).unwrap().is_err());
    assert_eq!(work(&app), before);
    assert_eq!(app.session.active().unwrap().history.past_len(), history);
}

fn panel(a: PhotocraftApp) -> egui_kittest::Harness<'static, PhotocraftApp> {
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(320.0, 360.0)).build_ui_state(|ui, a| crate::vector_ui::paths_panel(a, ui), a);
    PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ALL[0]);
    h.run_steps(4);
    h
}

#[test]
fn explicit_work_selection_over_active_shape_moves_whole_path_fractionally() {
    use egui_kittest::kittest::Queryable;
    let mut a = app();
    a.run("shape.create", json!({"kind":"rect","rect":[5,5,100,100],"fill":"#ff0000"})).unwrap();
    let mut h = panel(a);
    h.get_by_label("Work Path").click();
    h.run();
    let a = h.state_mut();
    assert!(selected_work(a));
    let before = work(a);
    let layers = a.session.active().unwrap().doc.layers.clone();
    let n = a.session.active().unwrap().history.past_len();
    down(a, [10.0, 20.0]);
    moving(a, [21.25, 24.5]);
    assert_eq!(preview(a), Some([11.25, 4.5]));
    assert_eq!(work(a), before);
    up(a, [21.25, 24.5]);
    assert_eq!(work(a), before.transform(&Affine::translate(11.25, 4.5)));
    assert_eq!(a.session.active().unwrap().doc.layers, layers);
    assert_eq!(a.session.active().unwrap().history.past_len(), n + 1);
    a.run("edit.undo", json!({})).unwrap();
    assert_eq!(work(a), before);
    a.run("edit.redo", json!({})).unwrap();
    assert_eq!(work(a), before.transform(&Affine::translate(11.25, 4.5)));
}

#[test]
fn work_selection_keeps_anchor_tools_on_work_over_active_shape_and_mask() {
    for mask in [false, true] {
        let mut a = app();
        if mask {
            a.run("layer.vectorMask.revealAll", json!({})).unwrap();
        } else {
            a.run("shape.create", json!({"kind":"rect","rect":[10,20,50,50]})).unwrap();
        }
        select_whole(&mut a);
        let before = work(&a);
        let layers = a.session.active().unwrap().doc.layers.clone();
        a.ui.tool = Tool::AddAnchorPoint;
        down(&mut a, [35.0, 20.0]);
        up(&mut a, [35.0, 20.0]);
        assert_eq!(work(&a).subpaths[0].knots.len(), before.subpaths[0].knots.len() + 1);
        a.ui.tool = Tool::DirectSelection;
        let p = work(&a).subpaths[0].knots[1].anchor;
        down(&mut a, [p.x, p.y]);
        moving(&mut a, [p.x, p.y - 10.0]);
        up(&mut a, [p.x, p.y - 10.0]);
        assert_eq!(work(&a).subpaths[0].knots[1].anchor.y, p.y - 10.0);
        assert_eq!(a.session.active().unwrap().doc.layers, layers);
        a.run("edit.undo", json!({})).unwrap();
        a.run("edit.undo", json!({})).unwrap();
        assert_eq!(work(&a), before);
        a.run("edit.redo", json!({})).unwrap();
        a.run("edit.redo", json!({})).unwrap();
        assert_eq!(work(&a).subpaths[0].knots[1].anchor.y, p.y - 10.0);
    }
}

#[test]
fn missed_or_hidden_work_paths_do_not_drag_or_fall_back() {
    for hidden in [false, true] {
        let mut a = app();
        select_whole(&mut a);
        a.ui.view.show.target_path = !hidden;
        let before = work(&a);
        let n = a.session.active().unwrap().history.past_len();
        let start = if hidden { [10.0, 20.0] } else { [350.0, 180.0] };
        down(&mut a, start);
        moving(&mut a, [start[0] + 10.0, start[1] + 10.0]);
        up(&mut a, [start[0] + 10.0, start[1] + 10.0]);
        assert_eq!(work(&a), before);
        assert_eq!(a.session.active().unwrap().history.past_len(), n);
    }
}

#[test]
fn every_component_hit_moves_the_whole_work_path() {
    for start in [[10.0, 20.0], [140.0, 20.0], [220.0, 40.0]] {
        let mut a = app();
        select_whole(&mut a);
        let before = work(&a);
        down(&mut a, start);
        moving(&mut a, [start[0] + 7.25, start[1] - 3.5]);
        up(&mut a, [start[0] + 7.25, start[1] - 3.5]);
        assert_eq!(work(&a), before.transform(&Affine::translate(7.25, -3.5)));
    }
}

#[test]
fn commands_and_target_switches_cancel_without_resurrecting_a_gesture() {
    for command in ["layer.new.layer", "edit.undo", "file.new"] {
        let mut a = app();
        select_whole(&mut a);
        down(&mut a, [10.0, 20.0]);
        moving(&mut a, [20.0, 25.0]);
        let params = if command == "file.new" { json!({"width":100,"height":100}) } else { json!({}) };
        a.run(command, params).unwrap();
        let doc = a.session.active().unwrap().doc.clone();
        let n = a.session.active().unwrap().history.past_len();
        assert!(preview(&a).is_none());
        assert!(finish(&mut a, [10.0, 20.0], [20.0, 25.0]).unwrap().is_err());
        assert_eq!(a.session.active().unwrap().doc, doc);
        assert_eq!(a.session.active().unwrap().history.past_len(), n);
    }
}

#[test]
fn explicit_work_target_hides_overlapping_shape_corner_widgets() {
    let mut a = app();
    a.run("shape.create", json!({"kind":"rect","rect":[0,10,100,100]})).unwrap();
    assert!(crate::corner_radius::hovers(&a, [10.0, 20.0]));
    select_whole(&mut a);
    assert!(!crate::corner_radius::hovers(&a, [10.0, 20.0]));
    let layers = a.session.active().unwrap().doc.layers.clone();
    down(&mut a, [10.0, 20.0]);
    moving(&mut a, [21.0, 24.0]);
    assert_eq!(preview(&a), Some([11.0, 4.0]));
    up(&mut a, [21.0, 24.0]);
    assert_eq!(a.session.active().unwrap().doc.layers, layers);
}

fn named_drag_app() -> PhotocraftApp {
    let mut a = app();
    for (name, y) in [("Path 1", 110), ("Path 2", 160)] {
        a.run("path.set", json!({"name":name,"path":{"subpaths":[{"closed":false,"knots":[[40,y],[160,y]]}]}})).unwrap();
    }
    a.ui.selected_path = Some("Path 1".into());
    a
}
fn saved(a: &PhotocraftApp, name: &str) -> Path {
    a.session.active().unwrap().doc.paths.iter().find(|p| p.name == name).unwrap().path.clone()
}
#[test]
fn named_drag_regression_does_not_move_work_or_other_saved_path() {
    let mut a = named_drag_app();
    let work0 = work(&a);
    let other = saved(&a, "Path 2");
    let before = saved(&a, "Path 1");
    down(&mut a, [100.0, 110.0]);
    moving(&mut a, [112.25, 117.5]);
    up(&mut a, [112.25, 117.5]);
    assert_eq!(work(&a), work0, "black-arrow named drag must not fall back to Work");
    assert_eq!(saved(&a, "Path 2"), other);
    assert_eq!(saved(&a, "Path 1"), before.transform(&Affine::translate(12.25, 7.5)));
}
#[test]
fn named_drag_regression_has_live_fractional_preview_before_commit() {
    let mut a = named_drag_app();
    let before = saved(&a, "Path 1");
    let n = a.session.active().unwrap().history.past_len();
    down(&mut a, [100.0, 110.0]);
    moving(&mut a, [112.25, 117.5]);
    assert_eq!(preview(&a), Some([12.25, 7.5]));
    assert_eq!(saved(&a, "Path 1"), before);
    assert_eq!(a.session.active().unwrap().history.past_len(), n);
}

#[test]
fn named_drag_each_selected_path_commits_once_and_roundtrips() {
    for name in ["Path 1", "Path 2", "work"] {
        let mut a = named_drag_app();
        if name != "work" {
            let compound = work(&a);
            a.run("path.set", json!({"name":name,"path":photocraft_engine::vector_cmds::path_json(&compound)})).unwrap();
        }
        a.ui.selected_path = Some(name.into());
        if name == "work" {
            select_whole(&mut a);
        }
        let doc = a.session.active().unwrap().doc.clone();
        let n = a.session.active().unwrap().history.past_len();
        let before = if name == "work" { work(&a) } else { saved(&a, name) };
        let start = [10.0, 20.0];
        down(&mut a, start);
        for delta in [[4.125, 3.25], [12.25, 7.5]] {
            moving(&mut a, [start[0] + delta[0], start[1] + delta[1]]);
            assert_eq!(preview(&a), Some(delta));
            assert_eq!(a.session.active().unwrap().doc, doc);
        }
        up(&mut a, [start[0] + 12.25, start[1] + 7.5]);
        let expected = before.transform(&Affine::translate(12.25, 7.5));
        if name == "work" {
            assert_eq!(work(&a), expected);
        } else {
            assert_eq!(saved(&a, name), expected);
            assert_eq!(work(&a), doc.work_path.clone().unwrap());
        }
        for path in &doc.paths {
            if path.name != name {
                assert_eq!(saved(&a, &path.name), path.path);
            }
        }
        assert_eq!(a.session.active().unwrap().history.past_len(), n + 1);
        let after = a.session.active().unwrap().doc.clone();
        a.run("edit.undo", json!({})).unwrap();
        assert_eq!(a.session.active().unwrap().doc, doc);
        a.run("edit.redo", json!({})).unwrap();
        assert_eq!(a.session.active().unwrap().doc, after);
    }
}
#[test]
fn named_drag_overlaps_use_only_explicit_selection_and_misses_do_nothing() {
    let mut a = named_drag_app();
    let same = saved(&a, "Path 1");
    a.run("path.set", json!({"name":"Path 2","path":photocraft_engine::vector_cmds::path_json(&same)})).unwrap();
    a.run("shape.create", json!({"kind":"rect","rect":[30,100,140,40]})).unwrap();
    a.ui.selected_path = Some("Path 2".into());
    let layers = a.session.active().unwrap().doc.layers.clone();
    let work0 = work(&a);
    assert!(!crate::corner_radius::hovers(&a, [40.0, 110.0]));
    down(&mut a, [40.0, 110.0]);
    moving(&mut a, [50.25, 117.5]);
    up(&mut a, [50.25, 117.5]);
    assert_eq!(saved(&a, "Path 1"), same);
    assert_eq!(saved(&a, "Path 2"), same.transform(&Affine::translate(10.25, 7.5)));
    assert_eq!(work(&a), work0);
    assert_eq!(a.session.active().unwrap().doc.layers, layers);
    let doc = a.session.active().unwrap().doc.clone();
    let n = a.session.active().unwrap().history.past_len();
    down(&mut a, [10.0, 20.0]);
    moving(&mut a, [30.0, 40.0]);
    up(&mut a, [30.0, 40.0]);
    assert_eq!(a.session.active().unwrap().doc, doc);
    assert_eq!(a.session.active().unwrap().history.past_len(), n);
}
#[test]
fn named_drag_stale_target_document_revision_and_geometry_never_commit() {
    for reason in ["selection", "revision", "geometry", "document", "rename", "delete", "tool", "focus"] {
        let mut a = named_drag_app();
        down(&mut a, [100.0, 110.0]);
        moving(&mut a, [112.25, 117.5]);
        match reason {
            "selection" => a.ui.selected_path = Some("Path 2".into()),
            "revision" => a.session.active_mut().unwrap().revision += 1,
            "geometry" => {
                std::sync::Arc::make_mut(&mut a.session.active_mut().unwrap().doc).paths[0].path = photocraft_vector::shapes::rect(2.0, 2.0, 10.0, 10.0)
            }
            "document" => {
                a.run("file.new", json!({"width":200,"height":200})).unwrap();
                a.run("path.set", json!({"name":"Path 1","path":{"subpaths":[{"knots":[[40,110],[160,110]]}]}})).unwrap();
            }
            "rename" => {
                a.run("path.rename", json!({"name":"Path 1","to":"Renamed"})).unwrap();
            }
            "delete" => {
                a.run("path.delete", json!({"name":"Path 1"})).unwrap();
            }
            "tool" => a.ui.tool = Tool::DirectSelection,
            "focus" => reconcile(&mut a, false),
            _ => {}
        }
        reconcile(&mut a, true);
        let doc = a.session.active().unwrap().doc.clone();
        let n = a.session.active().unwrap().history.past_len();
        assert!(preview(&a).is_none(), "{reason}");
        assert!(finish(&mut a, [100.0, 110.0], [112.25, 117.5]).unwrap().is_err(), "{reason}");
        assert_eq!(a.session.active().unwrap().doc, doc);
        assert_eq!(a.session.active().unwrap().history.past_len(), n);
    }
}
#[test]
fn named_drag_actual_panel_switch_cancels_even_if_original_row_is_reselected() {
    use egui_kittest::kittest::Queryable;
    let mut h = panel(named_drag_app());
    let doc = h.state().session.active().unwrap().doc.clone();
    down(h.state_mut(), [100.0, 110.0]);
    moving(h.state_mut(), [112.25, 117.5]);
    assert!(preview(h.state()).is_some());
    h.get_by_label("Path 2").click();
    h.run();
    h.get_by_label("Path 1").click();
    h.run();
    assert!(preview(h.state()).is_none());
    assert!(h.state().drag.is_none());
    assert!(finish(h.state_mut(), [100.0, 110.0], [112.25, 117.5]).unwrap().is_err());
    assert_eq!(h.state().session.active().unwrap().doc, doc);
}
#[test]
fn named_drag_primary_secondary_focus_and_escape_share_existing_lifecycle() {
    for cancel_kind in ["focus", "escape", "none"] {
        let mut a = named_drag_app();
        let before = saved(&a, "Path 1");
        down(&mut a, [100.0, 110.0]);
        moving(&mut a, [112.25, 117.5]);
        canvas_frame(&mut a, true, true);
        canvas_frame(&mut a, false, false);
        assert_eq!(preview(&a), Some([12.25, 7.5]));
        match cancel_kind {
            "focus" => canvas_frame(&mut a, true, false),
            "escape" => {
                let ctx = egui::Context::default();
                PhotocraftApp::setup_context(&ctx, crate::theme::ThemeKind::ALL[0]);
                let input = egui::RawInput {
                    events: vec![egui::Event::Key {
                        key: egui::Key::Escape,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }],
                    ..Default::default()
                };
                ctx.run_ui(input, |ui| crate::shortcuts::handle(&mut a, ui.ctx())).textures_delta.clear();
            }
            _ => {}
        }
        if cancel_kind == "none" {
            up(&mut a, [112.25, 117.5]);
            assert_eq!(saved(&a, "Path 1"), before.transform(&Affine::translate(12.25, 7.5)));
        } else {
            assert!(preview(&a).is_none());
            assert!(finish(&mut a, [100.0, 110.0], [112.25, 117.5]).unwrap().is_err());
            assert_eq!(saved(&a, "Path 1"), before);
        }
    }
}
#[test]
fn named_drag_overlay_moves_only_eligible_named_outline_and_anchor_squares() {
    let mut a = named_drag_app();
    down(&mut a, [100.0, 110.0]);
    let xf = crate::canvas::ViewXform {
        rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0)),
        zoom: 1.0,
        center: [200.0, 100.0],
        flip: false,
        rotation: 0.0,
        aspect: 1.0,
    };
    let before = overlay(&a, xf);
    moving(&mut a, [112.25, 117.5]);
    let during = overlay(&a, xf);
    let paths = |s: &[egui::Shape]| s.iter().filter_map(|s| if let egui::Shape::Path(p) = s { Some(p.points.clone()) } else { None }).collect::<Vec<_>>();
    let a0 = paths(&before);
    let a1 = paths(&during);
    assert_eq!(a0.len(), 1);
    assert_eq!(a1.len(), 1);
    for (i, (before, after)) in a0.iter().zip(&a1).enumerate() {
        for (p, q) in before.iter().zip(after) {
            let delta = if i == 0 { egui::vec2(12.25, 7.5) } else { egui::Vec2::ZERO };
            assert!((*q - *p - delta).length() < 0.001);
        }
    }
    let squares =
        |shapes: &[egui::Shape]| shapes.iter().filter_map(|s| if let egui::Shape::Rect(r) = s { Some(r.rect.center()) } else { None }).collect::<Vec<_>>();
    let b = squares(&before);
    let d = squares(&during);
    assert_eq!(b.len(), 4);
    assert_eq!(d.len(), 4);
    for (i, (p, q)) in b.iter().zip(&d).enumerate() {
        let delta = if i < 4 { egui::vec2(12.25, 7.5) } else { egui::Vec2::ZERO };
        assert!((*q - *p - delta).length() < 0.001);
    }
    // Only the selected named path is drawn with Show All Paths off.
    if let Some(dir) = std::env::var_os("PHOTOCRAFT_NAMED_DRAG_SNAPSHOT_DIR") {
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(800.0, 600.0)).build_ui_state(
            move |ui, a: &mut PhotocraftApp| {
                egui::CentralPanel::default().show(ui, |ui| crate::vector_ui::draw_overlay(a, ui.painter(), &xf, &a.session.active().unwrap().doc));
            },
            a,
        );
        PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ALL[0]);
        h.run_steps(3);
        h.render().unwrap().save(std::path::PathBuf::from(dir).join("named-live-drag.png")).unwrap();
    }
}
#[test]
fn named_drag_retains_existing_copied_path_text_geometry_and_identity() {
    let mut a = named_drag_app();
    let work0 = work(&a);
    let named = saved(&a, "Path 1");
    // Existing text attachment stores copied path geometry, not a live saved-name binding.
    a.run("path.set", json!({"name":"work","path":photocraft_engine::vector_cmds::path_json(&named)})).unwrap();
    let id = a.run("type.create", json!({"x":0,"y":0,"text":"Studio","size":16})).unwrap()["layer"].as_u64().unwrap();
    a.run("type.attachToPath", json!({"layer":id})).unwrap();
    a.run("path.set", json!({"name":"work","path":photocraft_engine::vector_cmds::path_json(&work0)})).unwrap();
    a.ui.selected_path = Some("Path 1".into());
    let layers = a.session.active().unwrap().doc.layers.clone();
    let pixels = photocraft_compose::flatten(&a.session.active().unwrap().doc);
    down(&mut a, [100.0, 110.0]);
    moving(&mut a, [112.25, 117.5]);
    up(&mut a, [112.25, 117.5]);
    assert_eq!(a.session.active().unwrap().doc.layers, layers);
    assert_eq!(work(&a), work0);
    assert_eq!(photocraft_compose::flatten(&a.session.active().unwrap().doc), pixels);
    assert_eq!(a.ui.selected_path.as_deref(), Some("Path 1"));
    a.run("edit.undo", json!({})).unwrap();
    a.run("edit.redo", json!({})).unwrap();
    assert_eq!(a.session.active().unwrap().doc.layers, layers);
}
#[test]
fn named_drag_missing_and_hidden_targets_cannot_move_work() {
    for name in ["Missing", "Path 1"] {
        let mut a = named_drag_app();
        a.ui.selected_path = Some(name.into());
        a.ui.view.show.target_path = false;
        let doc = a.session.active().unwrap().doc.clone();
        let n = a.session.active().unwrap().history.past_len();
        down(&mut a, [100.0, 110.0]);
        moving(&mut a, [112.25, 117.5]);
        up(&mut a, [112.25, 117.5]);
        assert_eq!(a.session.active().unwrap().doc, doc);
        assert_eq!(a.session.active().unwrap().history.past_len(), n);
    }
}

fn component_event(a: &mut PhotocraftApp, event: ToolEvent) {
    tool_event(a, event, egui::Modifiers::ALT);
}
fn component_down(a: &mut PhotocraftApp, p: [f64; 2]) {
    component_event(a, ToolEvent::Down { x: p[0], y: p[1], pressure: 1.0 });
}
fn component_move(a: &mut PhotocraftApp, p: [f64; 2]) {
    component_event(a, ToolEvent::Move { x: p[0], y: p[1], pressure: 1.0 });
}
fn component_up(a: &mut PhotocraftApp, p: [f64; 2]) {
    component_event(a, ToolEvent::Up { x: p[0], y: p[1] });
}
#[test]
fn component_regression_work_option_drag_moves_only_one_curve() {
    let mut a = app();
    select_whole(&mut a);
    let before = work(&a);
    component_down(&mut a, [220.0, 40.0]);
    component_move(&mut a, [231.25, 47.5]);
    component_up(&mut a, [231.25, 47.5]);
    let after = work(&a);
    assert_eq!(&after.subpaths[..2], &before.subpaths[..2], "unrelated components must stay fixed");
    let mut expected = before.clone();
    let anchors = photocraft_vector::edit::subpath_anchors(&expected, 2);
    photocraft_vector::edit::move_anchors(&mut expected, &anchors, [11.25, 7.5]).unwrap();
    assert_eq!(after, expected);
}
#[test]
fn component_regression_named_option_drag_retains_other_paths_and_components() {
    let mut a = named_drag_app();
    let original = work(&a);
    a.run("path.set", json!({"name":"Path 1","path":photocraft_engine::vector_cmds::path_json(&original)})).unwrap();
    let other = saved(&a, "Path 2");
    component_down(&mut a, [220.0, 40.0]);
    component_move(&mut a, [231.25, 47.5]);
    component_up(&mut a, [231.25, 47.5]);
    let after = saved(&a, "Path 1");
    assert_eq!(&after.subpaths[..2], &original.subpaths[..2]);
    assert_eq!(work(&a), original);
    assert_eq!(saved(&a, "Path 2"), other);
}

#[test]
fn component_each_subpath_work_and_named_has_one_history_step_and_unchanged_neighbours() {
    for named in [false, true] {
        for index in 0..3 {
            let mut a = app();
            select_whole(&mut a);
            let original = work(&a);
            if named {
                a.run("path.set", json!({"name":"Components","path":photocraft_engine::vector_cmds::path_json(&original)})).unwrap();
                a.ui.selected_path = Some("Components".into());
            }
            let target = PathRef::path(&a, if named { "Components" } else { "work" }).unwrap();
            let before = if named { saved(&a, "Components") } else { work(&a) };
            let n = a.session.active().unwrap().history.past_len();
            let layers = a.session.active().unwrap().doc.layers.clone();
            let start = [[10.0, 20.0], [140.0, 20.0], [220.0, 40.0]][index];
            component_down(&mut a, start);
            assert_eq!(selected_component(&a, &target), Some(index));
            component_move(&mut a, [start[0] + 12.25, start[1] + 7.5]);
            for i in 0..3 {
                assert_eq!(preview_for(&a, &target, i), (i == index).then_some([12.25, 7.5]));
            }
            assert_eq!(if named { saved(&a, "Components") } else { work(&a) }, before);
            component_up(&mut a, [start[0] + 12.25, start[1] + 7.5]);
            let after = if named { saved(&a, "Components") } else { work(&a) };
            assert_eq!(a.session.active().unwrap().history.past_len(), n + 1);
            assert_eq!(selected_component(&a, &target), Some(index));
            for (i, (old, new)) in before.subpaths.iter().zip(&after.subpaths).enumerate() {
                if i != index {
                    assert_eq!(old, new);
                } else {
                    assert_eq!(old.closed, new.closed);
                    assert_eq!(old.op, new.op);
                    for (k, l) in old.knots.iter().zip(&new.knots) {
                        assert_eq!(k.smooth, l.smooth);
                        for (p, q) in [(k.anchor, l.anchor), (k.in_ctrl, l.in_ctrl), (k.out_ctrl, l.out_ctrl)] {
                            assert_eq!([q.x - p.x, q.y - p.y], [12.25, 7.5]);
                        }
                    }
                }
            }
            assert_eq!(before.fill_rule, after.fill_rule);
            assert_eq!(before.inverted, after.inverted);
            assert_eq!(a.session.active().unwrap().doc.layers, layers);
            if named {
                assert_eq!(work(&a), original);
            }
            a.run("edit.undo", json!({})).unwrap();
            assert_eq!(if named { saved(&a, "Components") } else { work(&a) }, before);
            a.run("edit.redo", json!({})).unwrap();
            assert_eq!(if named { saved(&a, "Components") } else { work(&a) }, after);
        }
    }
}
#[test]
fn component_option_click_highlights_without_history_and_plain_drag_remains_whole() {
    let mut a = app();
    select_whole(&mut a);
    let before = work(&a);
    let n = a.session.active().unwrap().history.past_len();
    component_down(&mut a, [220.0, 40.0]);
    component_up(&mut a, [220.0, 40.0]);
    assert_eq!(selected_component(&a, &PathRef::Work), Some(2));
    assert_eq!(work(&a), before);
    assert_eq!(a.session.active().unwrap().history.past_len(), n);
    down(&mut a, [220.0, 40.0]);
    assert_eq!(selected_component(&a, &PathRef::Work), None);
    moving(&mut a, [231.25, 47.5]);
    up(&mut a, [231.25, 47.5]);
    assert_eq!(work(&a), before.transform(&Affine::translate(11.25, 7.5)));
}
#[test]
fn component_stale_in_range_index_document_target_and_revision_never_fall_back() {
    for reason in ["shorten", "revision", "document", "target", "invalid_index", "tool", "focus"] {
        let mut a = named_drag_app();
        let original = work(&a);
        a.run("path.set", json!({"name":"Path 1","path":photocraft_engine::vector_cmds::path_json(&original)})).unwrap();
        component_down(&mut a, [140.0, 20.0]);
        component_move(&mut a, [152.25, 27.5]);
        match reason {
            "shorten" => {
                let mut changed = original.clone();
                changed.subpaths.remove(0);
                a.run("path.set", json!({"name":"Path 1","path":photocraft_engine::vector_cmds::path_json(&changed)})).unwrap();
            }
            "revision" => a.session.active_mut().unwrap().revision += 1,
            "document" => {
                a.run("file.new", json!({"width":400,"height":200})).unwrap();
                a.run("path.set", json!({"name":"Path 1","path":photocraft_engine::vector_cmds::path_json(&original)})).unwrap();
            }
            "target" => a.ui.selected_path = Some("work".into()),
            "invalid_index" => a.ui.work_path_drag.selection.as_mut().unwrap().component = Some(usize::MAX),
            "tool" => a.ui.tool = Tool::DirectSelection,
            "focus" => reconcile(&mut a, false),
            _ => {}
        }
        reconcile(&mut a, true);
        let doc = a.session.active().unwrap().doc.clone();
        let n = a.session.active().unwrap().history.past_len();
        assert!(preview(&a).is_none(), "{reason}");
        assert!(finish(&mut a, [140.0, 20.0], [152.25, 27.5]).unwrap().is_err(), "{reason}");
        assert_eq!(a.session.active().unwrap().doc, doc);
        assert_eq!(a.session.active().unwrap().history.past_len(), n);
    }
}
#[test]
fn component_cancel_and_panel_reselection_clear_highlight_without_mutation() {
    use egui_kittest::kittest::Queryable;
    let mut a = app();
    select_whole(&mut a);
    let before = work(&a);
    component_down(&mut a, [220.0, 40.0]);
    component_move(&mut a, [230.0, 50.0]);
    cancel(&mut a);
    assert!(finish(&mut a, [220.0, 40.0], [230.0, 50.0]).unwrap().is_err());
    assert_eq!(work(&a), before);
    component_down(&mut a, [220.0, 40.0]);
    component_up(&mut a, [220.0, 40.0]);
    assert_eq!(selected_component(&a, &PathRef::Work), Some(2));
    let mut h = panel(a);
    h.get_by_label("Work Path").click();
    h.run();
    assert_eq!(selected_component(h.state(), &PathRef::Work), None);
    assert_eq!(work(h.state()), before);
}
#[test]
fn component_overlay_only_shifts_selected_subpath_and_preserves_other_targets() {
    let mut a = named_drag_app();
    let original = work(&a).transform(&Affine::translate(0.0, 90.0));
    a.run("path.set", json!({"name":"Path 1","path":photocraft_engine::vector_cmds::path_json(&original)})).unwrap();
    let target = PathRef::path(&a, "Path 1").unwrap();
    component_down(&mut a, [140.0, 110.0]);
    let xf = crate::canvas::ViewXform {
        rect: egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0)),
        zoom: 1.0,
        center: [200.0, 100.0],
        flip: false,
        rotation: 0.0,
        aspect: 1.0,
    };
    let before = overlay(&a, xf);
    component_move(&mut a, [160.0, 130.0]);
    let during = overlay(&a, xf);
    let paths = |s: &[egui::Shape]| s.iter().filter_map(|s| if let egui::Shape::Path(p) = s { Some(p.points.clone()) } else { None }).collect::<Vec<_>>();
    let old = paths(&before);
    let new = paths(&during);
    assert_eq!(old.len(), 3);
    assert_eq!(new.len(), 3);
    for (i, (p, q)) in old.iter().zip(&new).enumerate() {
        for (p, q) in p.iter().zip(q) {
            let delta = if i == 1 { egui::vec2(20.0, 20.0) } else { egui::Vec2::ZERO };
            assert!((*q - *p - delta).length() < 0.001);
        }
    }
    let squares = |s: &[egui::Shape]| s.iter().filter_map(|s| if let egui::Shape::Rect(r) = s { Some(r.rect.center()) } else { None }).collect::<Vec<_>>();
    let old = squares(&before);
    let new = squares(&during);
    assert_eq!(old.len(), 20);
    assert_eq!(new.len(), 20);
    for (i, (p, q)) in old.iter().zip(&new).enumerate() {
        let delta = if (8..16).contains(&i) { egui::vec2(20.0, 20.0) } else { egui::Vec2::ZERO };
        assert!((*q - *p - delta).length() < 0.001);
    }
    assert_eq!(selected_component(&a, &target), Some(1));
    if let Some(dir) = std::env::var_os("PHOTOCRAFT_COMPONENT_DRAG_SNAPSHOT_DIR") {
        let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(800.0, 600.0)).build_ui_state(
            move |ui, a: &mut PhotocraftApp| {
                egui::CentralPanel::default().show(ui, |ui| crate::vector_ui::draw_overlay(a, ui.painter(), &xf, &a.session.active().unwrap().doc));
            },
            a,
        );
        PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ALL[0]);
        h.run_steps(3);
        h.render().unwrap().save(std::path::PathBuf::from(dir).join("component-preview.png")).unwrap();
    }
}

#[test]
fn uncaptured_releases_preserve_status_and_never_fall_back() {
    for name in ["work", "Path 1", "Missing"] {
        for hidden in [false, true] {
            for moved in [false, true] {
                for component in [false, true] {
                    let mut a = named_drag_app();
                    a.ui.selected_path = Some(name.into());
                    a.ui.view.show.target_path = !hidden;
                    let start = if hidden { [100.0, 110.0] } else { [350.0, 180.0] };
                    let end = if moved { [start[0] + 12.0, start[1] + 7.0] } else { start };
                    let doc = a.session.active().unwrap().doc.clone();
                    let history = a.session.active().unwrap().history.past_len();
                    a.ui.status = "Ready".into();
                    a.ui.status_error = false;
                    if component {
                        component_down(&mut a, start);
                    } else {
                        down(&mut a, start);
                    }
                    assert!(owns_drag(&a));
                    assert!(a.ui.work_path_drag.gesture.is_none());
                    if component {
                        component_move(&mut a, end);
                        component_up(&mut a, end);
                    } else {
                        moving(&mut a, end);
                        up(&mut a, end);
                    }
                    assert_eq!(a.ui.status, "Ready", "{name}, hidden={hidden}, moved={moved}, component={component}");
                    assert!(!a.ui.status_error);
                    assert_eq!(a.session.active().unwrap().doc, doc);
                    assert_eq!(a.session.active().unwrap().history.past_len(), history);
                    assert!(!owns_drag(&a));
                }
            }
        }
    }
}

#[test]
fn captured_cancellation_reports_error_and_next_miss_is_quiet() {
    let mut a = app();
    down(&mut a, [10.0, 20.0]);
    cancel(&mut a);
    crate::vector_ui::path_selection_finish(&mut a, [10.0, 20.0], [22.0, 27.0]);
    assert_eq!(a.ui.status, "Path drag was cancelled.");
    assert!(a.ui.status_error);
    assert!(!owns_drag(&a));
    // A new missed press clears cancellation bookkeeping but preserves existing status.
    down(&mut a, [350.0, 180.0]);
    assert_eq!(finish(&mut a, [350.0, 180.0], [362.0, 187.0]), Some(Ok(())));
    assert_eq!(a.ui.status, "Path drag was cancelled.");
    assert!(a.ui.status_error);
}

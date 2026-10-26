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
fn named_paths_shape_layers_and_masks_retain_their_existing_route() {
    let mut app = app();
    app.run("path.set", json!({"name": "Saved", "path": {"subpaths": [{"knots": [[5,5],[15,15]]}]}})).unwrap();
    app.ui.selected_path = Some("Saved".into());
    let before = app.session.active().unwrap().doc.clone();
    press(&mut app, [10.0, 20.0]);
    assert!(preview(&app).is_none());
    assert!(finish(&mut app, [10.0, 20.0], [20.0, 20.0]).is_none());
    assert_eq!(app.ui.selected_path.as_deref(), Some("Saved"));
    assert_eq!(app.session.active().unwrap().doc, before);
    app.ui.selected_path = Some("work".into());
    app.run("layer.vectorMask.revealAll", json!({})).unwrap();
    press(&mut app, [10.0, 20.0]);
    assert!(finish(&mut app, [10.0, 20.0], [20.0, 20.0]).is_none());
    app.ui.vector_mask_target = false;
    app.run("shape.create", json!({"kind":"rect", "rect":[10,20,40,40]})).unwrap();
    press(&mut app, [10.0, 20.0]);
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

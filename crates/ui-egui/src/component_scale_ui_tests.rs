use super::*;
use crate::canvas::tool_event;
use photocraft_doc::Path;
pub(super) fn fixture(name: &str) -> PhotocraftApp {
    let mut a = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
    a.run("file.new", json!({"width":400,"height":300})).unwrap();
    a.sync_views();
    let mut p = photocraft_vector::shapes::rect(10.0, 20.0, 40.0, 50.0);
    p.subpaths.extend(photocraft_vector::shapes::ellipse(100.0, 80.0, 80.0, 60.0).subpaths);
    p.subpaths.push(photocraft_doc::vector::Subpath::polyline(&[(230.0, 180.0), (290.0, 220.0)]));
    for entry in ["work", "One", "Two"] {
        let geometry =
            if entry == name { p.clone() } else { p.transform(&photocraft_geom::Affine::translate(0.0, if entry == "Two" { 100.0 } else { -60.0 })) };
        a.run("path.set", json!({"name":entry,"path":photocraft_engine::vector_cmds::path_json(&geometry)})).unwrap();
    }
    a.ui.tool = Tool::PathSelection;
    a.ui.selected_path = Some(name.into());
    if name == "work" {
        crate::work_path_drag::select_whole(&mut a);
    }
    tool_event(&mut a, ToolEvent::Down { x: 140.0, y: 80.0, pressure: 1.0 }, egui::Modifiers::ALT);
    tool_event(&mut a, ToolEvent::Up { x: 140.0, y: 80.0 }, egui::Modifiers::ALT);
    a
}
pub(super) fn path(a: &PhotocraftApp, name: &str) -> Path {
    let doc = &a.session.active().unwrap().doc;
    if name == "work" { doc.work_path.clone().unwrap() } else { doc.paths.iter().find(|p| p.name == name).unwrap().path.clone() }
}
#[test]
fn component_scale_ui_tests_cmd_t_keeps_work_or_named_component_scope_and_bounds() {
    for name in ["work", "One"] {
        let mut a = fixture(name);
        let ctx = egui::Context::default();
        crate::menus::invoke(&mut a, &ctx, "edit.freeTransform", json!({})).unwrap();
        let t = a.ui.transform.as_ref().unwrap();
        assert_eq!(t.path.as_ref().unwrap()["name"], name);
        assert_eq!(t.path.as_ref().unwrap()["subpaths"], json!([1]));
        assert_eq!(t.rect, [100.0, 80.0, 180.0, 140.0]);
        assert_eq!(t.pivot, [140.0, 110.0]);
    }
}
#[test]
fn component_scale_ui_tests_handle_resize_changes_only_one_component_and_roundtrips() {
    for name in ["work", "One"] {
        let mut a = fixture(name);
        let original = path(&a, name);
        let other = path(&a, "Two");
        let work = path(&a, "work");
        let history = a.session.active().unwrap().history.past_len();
        begin(&mut a, &egui::Context::default()).unwrap();
        pointer(&mut a, ToolEvent::Down { x: 180.0, y: 140.0, pressure: 1.0 }, egui::Modifiers::SHIFT);
        pointer(&mut a, ToolEvent::Move { x: 220.0, y: 170.0, pressure: 1.0 }, egui::Modifiers::SHIFT);
        pointer(&mut a, ToolEvent::Up { x: 220.0, y: 170.0 }, egui::Modifiers::SHIFT);
        assert_eq!(path(&a, name), original, "preview cannot mutate document");
        commit(&mut a);
        assert!(a.ui.transform.is_none(), "{}", a.ui.status);
        let moved = path(&a, name);
        assert_eq!(moved.subpaths[0], original.subpaths[0]);
        assert_eq!(moved.subpaths[2], original.subpaths[2]);
        assert_ne!(moved.subpaths[1], original.subpaths[1]);
        assert_eq!(path(&a, "Two"), other);
        if name != "work" {
            assert_eq!(path(&a, "work"), work);
        }
        assert_eq!(a.session.active().unwrap().history.past_len(), history + 1);
        a.run("edit.undo", json!({})).unwrap();
        assert_eq!(path(&a, name), original);
        a.run("edit.redo", json!({})).unwrap();
        assert_eq!(path(&a, name), moved);
    }
}

fn resize(a: &mut PhotocraftApp) {
    let t = a.ui.transform.as_mut().unwrap();
    t.quad = [[100.0, 80.0], [220.0, 80.0], [220.0, 170.0], [100.0, 170.0]];
}
#[test]
fn component_scale_ui_tests_stale_selection_document_revision_layer_tool_and_geometry_cancel() {
    for name in ["work", "One"] {
        for reason in ["row", "clear", "revision", "geometry", "document", "layer", "tool", "reselect"] {
            for via_frame in [false, true] {
                let mut a = fixture(name);
                begin(&mut a, &egui::Context::default()).unwrap();
                resize(&mut a);
                match reason {
                    "row" => a.ui.selected_path = Some("Two".into()),
                    "clear" => a.ui.selected_path = None,
                    "revision" => a.session.active_mut().unwrap().revision += 1,
                    "geometry" => {
                        let d = Arc::make_mut(&mut a.session.active_mut().unwrap().doc);
                        if name == "work" {
                            d.work_path.as_mut().unwrap().subpaths.remove(0);
                        } else {
                            d.paths.iter_mut().find(|p| p.name == name).unwrap().path.subpaths.remove(0);
                        }
                    }
                    "document" => {
                        a.session.execute("file.new", json!({"width":100,"height":100})).unwrap();
                    }
                    "layer" => a.session.active_mut().unwrap().active_layer = None,
                    "tool" => a.ui.tool = Tool::DirectSelection,
                    _ => crate::work_path_drag::reset_scope(&mut a),
                }
                let docs = a.session.documents().iter().map(|s| (s.doc.clone(), s.history.past_len())).collect::<Vec<_>>();
                if via_frame {
                    end_if_left(&mut a);
                } else {
                    commit(&mut a);
                }
                assert!(a.ui.transform.is_none(), "{name} {reason}");
                assert!(a.transform_preview.is_none());
                assert_eq!(a.session.documents().iter().map(|s| (s.doc.clone(), s.history.past_len())).collect::<Vec<_>>(), docs, "{name} {reason}");
            }
        }
    }
}
#[test]
fn component_scale_ui_tests_invalidated_scope_never_falls_back_but_row_reselection_restores_whole() {
    for name in ["work", "One"] {
        let mut a = fixture(name);
        a.run("path.transform", json!({"name":"Two","translateX":1})).unwrap();
        assert!(begin(&mut a, &egui::Context::default()).is_err());
        assert!(a.ui.transform.is_none());
        if name == "work" {
            crate::work_path_drag::select_whole(&mut a);
        } else {
            crate::work_path_drag::reset_scope(&mut a);
        }
        begin(&mut a, &egui::Context::default()).unwrap();
        let t = a.ui.transform.as_ref().unwrap();
        assert_eq!(t.path.as_ref().unwrap(), &json!({"name":name}));
        assert_eq!(t.rect, [10.0, 20.0, 290.0, 220.0]);
        assert!(!component_restricts(&a, "edit.transform.rotate"));
        cancel(&mut a);
    }
}
#[test]
fn component_scale_ui_tests_real_panel_reselect_clears_scope_without_stale_commit() {
    use egui_kittest::kittest::Queryable;
    for name in ["work", "One"] {
        let mut a = fixture(name);
        let original = path(&a, name);
        begin(&mut a, &egui::Context::default()).unwrap();
        resize(&mut a);
        let mut h =
            egui_kittest::Harness::builder().with_size(vec2(320.0, 400.0)).build_ui_state(|ui, a: &mut PhotocraftApp| crate::vector_ui::paths_panel(a, ui), a);
        PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ALL[0]);
        h.run_steps(3);
        h.get_by_label(if name == "work" { "Work Path" } else { name }).click();
        h.run_steps(2);
        end_if_left(h.state_mut());
        assert!(h.state().ui.transform.is_none());
        assert_eq!(path(h.state(), name), original);
        begin(h.state_mut(), &egui::Context::default()).unwrap();
        assert!(h.state().ui.transform.as_ref().unwrap().path.as_ref().unwrap().get("subpaths").is_none());
    }
}
#[test]
fn component_scale_ui_tests_unsupported_modes_presets_warp_and_copy_are_refused() {
    for opened in [false, true] {
        for id in [
            "edit.transform.skew",
            "edit.transform.distort",
            "edit.transform.perspective",
            "edit.transform.warp",
            "edit.transform.flipHorizontal",
            "edit.freeTransformCopy",
            "layer.smartObjects.warp",
            "edit.contentAwareScale",
            "edit.puppetWarp",
            "select.transformSelection",
        ] {
            let mut a = fixture("One");
            let ctx = egui::Context::default();
            if opened {
                begin(&mut a, &ctx).unwrap();
            }
            let doc = a.session.active().unwrap().doc.clone();
            let t = a.ui.transform.clone();
            assert!(!crate::menus::is_enabled(&a, id), "{id}");
            assert!(crate::menus::invoke(&mut a, &ctx, id, json!({})).is_err(), "{id}");
            assert!(a.run(id, json!({})).is_err(), "{id}");
            assert_eq!(a.session.active().unwrap().doc, doc);
            assert_eq!(a.ui.transform, t);
        }
    }
    let mut a = fixture("One");
    begin(&mut a, &egui::Context::default()).unwrap();
    let t = a.ui.transform.clone();
    enter_warp(&mut a, &egui::Context::default());
    assert_eq!(a.ui.transform, t);
}
#[test]
fn component_scale_ui_tests_distort_modifiers_and_reflection_stay_safe() {
    for mods in [egui::Modifiers::COMMAND, egui::Modifiers { command: true, alt: true, shift: true, ..Default::default() }] {
        let mut a = fixture("One");
        begin(&mut a, &egui::Context::default()).unwrap();
        pointer(&mut a, ToolEvent::Down { x: 180.0, y: 140.0, pressure: 1.0 }, mods);
        pointer(&mut a, ToolEvent::Move { x: 220.0, y: 160.0, pressure: 1.0 }, mods);
        assert!(component_matrix(a.ui.transform.as_ref().unwrap()).is_some());
        assert!(!corner_distorts(&a, 2, mods));
    }
    let mut a = fixture("work");
    begin(&mut a, &egui::Context::default()).unwrap();
    let q = a.ui.transform.as_ref().unwrap().quad;
    pointer(&mut a, ToolEvent::Down { x: 180.0, y: 140.0, pressure: 1.0 }, egui::Modifiers::SHIFT);
    pointer(&mut a, ToolEvent::Up { x: 20.0, y: 10.0 }, egui::Modifiers::SHIFT);
    assert_eq!(a.ui.transform.as_ref().unwrap().quad, q);
    scale_about_pivot(&mut a, -1.0, 1.0);
    assert_eq!(a.ui.transform.as_ref().unwrap().quad, q);
    scale_about_pivot(&mut a, 0.0, 1.0);
    assert_eq!(a.ui.transform.as_ref().unwrap().quad, q);
    scale_about_pivot(&mut a, 1.25, 0.5);
    assert!(component_matrix(a.ui.transform.as_ref().unwrap()).is_some());
    assert_ne!(a.ui.transform.as_ref().unwrap().quad, q);
}
#[test]
fn component_scale_ui_tests_preview_matches_commit_handles_and_context_stays_stationary() {
    for name in ["work", "One"] {
        for all in [false, true] {
            let mut a = fixture(name);
            a.ui.show_all_paths = all;
            let original = path(&a, name);
            begin(&mut a, &egui::Context::default()).unwrap();
            resize(&mut a);
            let t = a.ui.transform.as_ref().unwrap().clone();
            let m = component_matrix(&t).unwrap();
            let preview = a.transform_preview.as_ref().unwrap().path.as_ref().unwrap().transform(&m);
            let siblings = component_siblings(&a, &a.session.active().unwrap().doc).unwrap();
            assert_eq!(siblings.subpaths, vec![original.subpaths[0].clone(), original.subpaths[2].clone()]);
            assert_eq!(path(&a, name), original);
            commit(&mut a);
            assert_eq!(path(&a, name).subpaths[1], preview.subpaths[0]);
            for (before, after) in original.subpaths[1].knots.iter().zip(&path(&a, name).subpaths[1].knots) {
                assert_eq!(before.smooth, after.smooth);
            }
            assert!(crate::work_path_drag::component_transform(&a).unwrap().is_some(), "successful scaling retains fresh component scope");
        }
    }
}
#[test]
fn component_scale_ui_tests_box_history_cancel_and_document_branching_are_independent() {
    let mut a = fixture("One");
    let before = path(&a, "One");
    let history = a.session.active().unwrap().history.past_len();
    begin(&mut a, &egui::Context::default()).unwrap();
    resize(&mut a);
    track_steps(&mut a, &egui::Context::default());
    a.run("edit.undo", json!({})).unwrap();
    assert_eq!(a.ui.transform.as_ref().unwrap().quad, corners([100.0, 80.0, 180.0, 140.0]));
    a.run("edit.redo", json!({})).unwrap();
    assert_eq!(a.ui.transform.as_ref().unwrap().quad[2], [220.0, 170.0]);
    cancel(&mut a);
    assert_eq!(path(&a, "One"), before);
    assert_eq!(a.session.active().unwrap().history.past_len(), history);
    begin(&mut a, &egui::Context::default()).unwrap();
    resize(&mut a);
    commit(&mut a);
    for _ in 0..3 {
        a.run("edit.undo", json!({})).unwrap();
        assert_eq!(path(&a, "One"), before);
        a.run("edit.redo", json!({})).unwrap();
    }
    a.run("edit.undo", json!({})).unwrap();
    assert!(begin(&mut a, &egui::Context::default()).is_err(), "undo invalidates old component scope");
    tool_event(&mut a, ToolEvent::Down { x: 140.0, y: 80.0, pressure: 1.0 }, egui::Modifiers::ALT);
    tool_event(&mut a, ToolEvent::Up { x: 140.0, y: 80.0 }, egui::Modifiers::ALT);
    begin(&mut a, &egui::Context::default()).unwrap();
    scale_about_pivot(&mut a, 1.2, 1.2);
    commit(&mut a);
    assert_eq!(a.session.active().unwrap().history.redo_labels().count(), 0);
}
#[test]
fn component_scale_ui_tests_missing_named_refused_and_straight_component_retains_geometry_pivot() {
    let mut a = fixture("One");
    a.ui.selected_path = Some("Missing".into());
    assert!(begin(&mut a, &egui::Context::default()).is_err());
    assert!(a.ui.transform.is_none());
    let mut a = fixture("work");
    let mut path = path(&a, "work");
    path.subpaths[1] = photocraft_doc::vector::Subpath::polyline(&[(100.0, 80.0), (180.0, 80.0)]);
    a.run("path.set", json!({"name":"work","path":photocraft_engine::vector_cmds::path_json(&path)})).unwrap();
    tool_event(&mut a, ToolEvent::Down { x: 140.0, y: 80.0, pressure: 1.0 }, egui::Modifiers::ALT);
    tool_event(&mut a, ToolEvent::Up { x: 140.0, y: 80.0 }, egui::Modifiers::ALT);
    begin(&mut a, &egui::Context::default()).unwrap();
    assert_eq!(a.ui.transform.as_ref().unwrap().rect, [100.0, 79.5, 180.0, 80.5]);
    assert_eq!(a.ui.transform.as_ref().unwrap().pivot, [140.0, 80.0]);
    scale_about_pivot(&mut a, 2.0, 2.0);
    commit(&mut a);
    let p = super::component_scale_ui_tests::path(&a, "work");
    assert_eq!(p.subpaths[1].knots[0].anchor, photocraft_geom::Point::new(60.0, 80.0));
    assert_eq!(p.subpaths[1].knots[1].anchor, photocraft_geom::Point::new(220.0, 80.0));
}

#[test]
fn component_scale_ui_tests_keyboard_cmd_t_enter_and_escape_use_existing_routes() {
    let a = fixture("One");
    let original = path(&a, "One");
    let mut h = egui_kittest::Harness::builder().with_size(vec2(800.0, 600.0)).build_ui_state(
        |ui, a: &mut PhotocraftApp| {
            crate::shortcuts::handle(a, ui.ctx());
            egui::CentralPanel::default().show(ui, |ui| {
                ui.label("Canvas keyboard focus");
            });
        },
        a,
    );
    PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ALL[0]);
    h.run_steps(2);
    fn key(h: &mut egui_kittest::Harness<PhotocraftApp>, key: egui::Key, mods: egui::Modifiers) {
        h.event(egui::Event::Key { key, physical_key: None, pressed: true, repeat: false, modifiers: mods });
        h.run_steps(1);
        h.event(egui::Event::Key { key, physical_key: None, pressed: false, repeat: false, modifiers: mods });
        h.run_steps(1);
    }
    key(&mut h, egui::Key::T, egui::Modifiers::COMMAND);
    assert!(h.state().ui.transform.is_some());
    resize(h.state_mut());
    key(&mut h, egui::Key::Escape, egui::Modifiers::NONE);
    assert!(h.state().ui.transform.is_none());
    assert_eq!(path(h.state(), "One"), original);
    key(&mut h, egui::Key::T, egui::Modifiers::COMMAND);
    resize(h.state_mut());
    key(&mut h, egui::Key::Enter, egui::Modifiers::NONE);
    assert!(h.state().ui.transform.is_none());
    assert_ne!(path(h.state(), "One").subpaths[1], original.subpaths[1]);
}
#[test]
fn component_scale_ui_tests_whole_named_rotation_and_component_first_last_open_paths() {
    let mut a = fixture("One");
    crate::work_path_drag::reset_scope(&mut a);
    begin(&mut a, &egui::Context::default()).unwrap();
    assert_eq!(a.ui.transform.as_ref().unwrap().path, Some(json!({"name":"One"})));
    assert!(!component_restricts(&a, "edit.transform.rotate90Cw"));
    rotate_about_pivot(&mut a, std::f64::consts::FRAC_PI_2);
    let original = path(&a, "One");
    commit(&mut a);
    assert_ne!(path(&a, "One"), original);
    for name in ["work", "One"] {
        for (index, at) in [(0, [10.0, 20.0]), (1, [140.0, 80.0]), (2, [230.0, 180.0])] {
            let mut a = fixture(name);
            let original = path(&a, name);
            tool_event(&mut a, ToolEvent::Down { x: at[0], y: at[1], pressure: 1.0 }, egui::Modifiers::ALT);
            tool_event(&mut a, ToolEvent::Up { x: at[0], y: at[1] }, egui::Modifiers::ALT);
            begin(&mut a, &egui::Context::default()).unwrap();
            scale_about_pivot(&mut a, 1.1, 0.9);
            commit(&mut a);
            let p = path(&a, name);
            for i in 0..3 {
                if i != index {
                    assert_eq!(p.subpaths[i], original.subpaths[i]);
                } else {
                    assert_ne!(p.subpaths[i], original.subpaths[i]);
                    assert_eq!(p.subpaths[i].closed, original.subpaths[i].closed);
                }
            }
        }
    }
}
#[test]
fn component_scale_ui_tests_overlay_shows_unmoved_siblings_and_optional_reference_paths() {
    for all in [false, true] {
        let mut a = fixture("One");
        a.ui.show_all_paths = all;
        begin(&mut a, &egui::Context::default()).unwrap();
        resize(&mut a);
        let xf = ViewXform {
            rect: egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(800.0, 600.0)),
            zoom: 1.0,
            center: [160.0, 150.0],
            flip: false,
            rotation: 0.0,
            aspect: 1.0,
        };
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let p = ui.ctx().layer_painter(egui::LayerId::background());
            crate::vector_ui::draw_overlay(&a, &p, &xf, &a.session.active().unwrap().doc);
            draw_overlay(&a, &p, &xf);
        });
        output.textures_delta.clear();
        let paths = output.shapes.iter().filter_map(|s| if let egui::Shape::Path(p) = &s.shape { Some(p.points.clone()) } else { None }).collect::<Vec<_>>();
        assert_eq!(paths.len(), if all { 10 } else { 4 });
        for (points, _) in
            crate::vector_ui::path_lines(&component_siblings(&a, &a.session.active().unwrap().doc).unwrap(), &|p| xf.to_screen(p[0] as f32, p[1] as f32))
        {
            assert_eq!(paths.iter().filter(|p| **p == points).count(), 1, "stationary sibling shown exactly once");
        }
        if all && let Some(dir) = std::env::var_os("PHOTOCRAFT_COMPONENT_SCALE_SNAPSHOT_DIR") {
            let mut h = egui_kittest::Harness::builder().with_size(vec2(1000.0, 600.0)).build_ui_state(
                |ui, a: &mut PhotocraftApp| {
                    egui::Panel::left("paths").exact_size(220.0).show(ui, |ui| crate::vector_ui::paths_panel(a, ui));
                    egui::Panel::top("scale-options").exact_size(35.0).show(ui, |ui| options_bar(a, ui));
                    egui::CentralPanel::default().show(ui, |ui| {
                        let xf = ViewXform { rect: ui.max_rect(), zoom: 1.0, center: [160.0, 150.0], flip: false, rotation: 0.0, aspect: 1.0 };
                        crate::vector_ui::draw_overlay(a, ui.painter(), &xf, &a.session.active().unwrap().doc);
                        draw_overlay(a, ui.painter(), &xf);
                    });
                },
                a,
            );
            PhotocraftApp::setup_context(&h.ctx, crate::theme::ThemeKind::ALL[0]);
            h.run_steps(3);
            h.render().unwrap().save(std::path::PathBuf::from(dir).join("component-scale-preview.png")).unwrap();
        }
    }
}

#[test]
fn component_scale_ui_tests_ordinary_drag_after_component_click_keeps_whole_transform_scope() {
    let mut a = fixture("One");
    tool_event(&mut a, ToolEvent::Down { x: 10.0, y: 20.0, pressure: 1.0 }, egui::Modifiers::NONE);
    tool_event(&mut a, ToolEvent::Up { x: 20.0, y: 30.0 }, egui::Modifiers::NONE);
    begin(&mut a, &egui::Context::default()).unwrap();
    assert_eq!(a.ui.transform.as_ref().unwrap().path, Some(json!({"name":"One"})));
    assert_eq!(a.ui.transform.as_ref().unwrap().rect, [20.0, 30.0, 300.0, 230.0]);
}

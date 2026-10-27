use super::*;

fn fixture() -> Session {
    let mut s = Session::new();
    s.execute("file.new", json!({"width":400,"height":300})).unwrap();
    let mut path = photocraft_vector::shapes::rect(10.0, 20.0, 40.0, 50.0);
    path.subpaths.extend(photocraft_vector::shapes::ellipse(100.0, 80.0, 80.0, 60.0).subpaths);
    path.subpaths.push(photocraft_doc::vector::Subpath::polyline(&[(220.0, 20.0), (280.0, 80.0)]));
    for name in ["work", "One", "Two"] {
        s.execute("path.set", json!({"name":name,"path":path_json(&path)})).unwrap();
    }
    s
}
fn request(s: &Session, name: &str, index: usize) -> Value {
    let st = s.active().unwrap();
    json!({"name":name,"subpaths":[index],"document":st.doc.id.0,"revision":st.revision,"matrix":[1.5,0,0,0.5,3,-2]})
}
fn target(s: &Session, name: &str) -> Path {
    transform_target(s, name, None).unwrap()
}
fn reject(s: &mut Session, p: Value) {
    let before = s.active().unwrap().doc.clone();
    let st = s.active().unwrap();
    let revision = st.revision;
    let history = st.history.past_len();
    let redo = st.history.redo_labels().map(str::to_owned).collect::<Vec<_>>();
    assert!(s.execute("path.transform", p.clone()).is_err(), "must reject {p}");
    let st = s.active().unwrap();
    assert_eq!(st.doc, before);
    assert_eq!(st.revision, revision);
    assert_eq!(st.history.past_len(), history);
    assert_eq!(st.history.redo_labels().map(str::to_owned).collect::<Vec<_>>(), redo);
}
#[test]
fn component_transform_tests_scaling_work_and_named_changes_only_requested_component() {
    for name in ["work", "One"] {
        let mut s = fixture();
        let original = target(&s, name);
        let other = target(&s, "Two");
        let work = target(&s, "work");
        let saved = target(&s, "One");
        let p = request(&s, name, 1);
        let history = s.active().unwrap().history.past_len();
        s.execute("path.transform", p).unwrap();
        let after = target(&s, name);
        assert_eq!(after.subpaths[0], original.subpaths[0]);
        assert_eq!(after.subpaths[2], original.subpaths[2]);
        let expected = original.transform(&Affine { m: [1.5, 0.0, 0.0, 0.5, 3.0, -2.0] });
        assert_eq!(after.subpaths[1], expected.subpaths[1]);
        assert_eq!(after.fill_rule, original.fill_rule);
        assert_eq!(after.inverted, original.inverted);
        assert_eq!(target(&s, "Two"), other);
        if name == "work" {
            assert_eq!(target(&s, "One"), saved)
        } else {
            assert_eq!(target(&s, "work"), work)
        }
        assert_eq!(s.active().unwrap().history.past_len(), history + 1);
        for _ in 0..3 {
            s.execute("edit.undo", json!({})).unwrap();
            assert_eq!(target(&s, name), original);
            s.execute("edit.redo", json!({})).unwrap();
            assert_eq!(target(&s, name), after);
        }
    }
}
#[test]
fn component_transform_tests_stale_revision_and_invalid_indices_never_fall_back() {
    let mut s = fixture();
    let mut p = request(&s, "work", 1);
    p["revision"] = json!(s.active().unwrap().revision + 1);
    reject(&mut s, p);
    for i in [3, usize::MAX] {
        let p = request(&s, "One", i);
        reject(&mut s, p);
    }
}

#[test]
fn component_transform_tests_numeric_pivot_is_component_local_uniform_and_nonuniform() {
    for (sx, sy) in [(2.0, 2.0), (2.0, 0.5)] {
        let mut s = fixture();
        let original = target(&s, "One");
        let mut p = request(&s, "One", 1);
        p.as_object_mut().unwrap().remove("matrix");
        p["scaleX"] = json!(sx);
        p["scaleY"] = json!(sy);
        s.execute("path.transform", p).unwrap();
        let after = target(&s, "One");
        // Ellipse control bounds centre is (140,110), distinct from the whole compound bounds.
        for (old, new) in original.subpaths[1].knots.iter().zip(&after.subpaths[1].knots) {
            for (old, new) in [(old.anchor, new.anchor), (old.in_ctrl, new.in_ctrl), (old.out_ctrl, new.out_ctrl)] {
                assert!((new.x - (140.0 + (old.x - 140.0) * sx)).abs() < 1e-9);
                assert!((new.y - (110.0 + (old.y - 110.0) * sy)).abs() < 1e-9);
            }
        }
        assert!(after.subpaths[1].closed);
        assert_eq!(after.subpaths[1].knots.len(), 4);
    }
}
#[test]
fn component_transform_tests_each_open_closed_component_keeps_structure_and_metadata() {
    for name in ["work", "One"] {
        for i in 0..3 {
            let mut s = fixture();
            let mut base = target(&s, name);
            base.fill_rule = photocraft_doc::vector::FillRule::EvenOdd;
            base.inverted = true;
            base.subpaths[i].op = photocraft_doc::vector::PathOp::Subtract;
            s.execute("path.set", json!({"name":name,"path":path_json(&base)})).unwrap();
            let p = request(&s, name, i);
            s.execute("path.transform", p).unwrap();
            let after = target(&s, name);
            assert_eq!(after.fill_rule, base.fill_rule);
            assert_eq!(after.inverted, base.inverted);
            for (j, (old, new)) in base.subpaths.iter().zip(&after.subpaths).enumerate() {
                if i != j {
                    assert_eq!(old, new);
                } else {
                    assert_eq!(old.closed, new.closed);
                    assert_eq!(old.op, new.op);
                    assert_eq!(old.knots.len(), new.knots.len());
                    for (o, n) in old.knots.iter().zip(&new.knots) {
                        assert_eq!(o.smooth, n.smooth);
                    }
                }
            }
        }
    }
}
#[test]
fn component_transform_tests_malformed_or_incomplete_selectors_never_fall_back() {
    let mut s = fixture();
    let base = request(&s, "work", 1);
    for key in ["name", "subpaths", "document", "revision"] {
        let mut p = base.clone();
        p.as_object_mut().unwrap().remove(key);
        reject(&mut s, p);
    }
    for value in [json!(null), json!([]), json!([0, 1]), json!([-1]), json!([1.0]), json!(["1"]), json!({}), json!(false)] {
        let mut p = base.clone();
        p["subpaths"] = value;
        reject(&mut s, p);
    }
    for key in ["name", "document", "revision"] {
        for value in [json!(null), json!(false), json!([]), json!(-1)] {
            let mut p = base.clone();
            p[key] = value;
            reject(&mut s, p);
        }
    }
    for name in ["", "layer", "Work Path", "WORK", "Missing"] {
        let mut p = base.clone();
        p["name"] = json!(name);
        reject(&mut s, p);
    }
}
#[test]
fn component_transform_tests_wrong_document_and_missing_work_do_not_target_layer() {
    let mut s = fixture();
    let p = request(&s, "One", 1);
    let old = s.active().unwrap().doc.clone();
    s.execute("file.new", json!({"width":100,"height":100})).unwrap();
    s.execute("shape.create", json!({"kind":"rect","rect":[10,10,50,50]})).unwrap();
    reject(&mut s, p);
    let p = request(&s, "work", 0);
    reject(&mut s, p);
    assert_eq!(s.documents()[0].doc, old);
}
#[test]
fn component_transform_tests_rejects_unsupported_and_degenerate_geometry_preserving_redo() {
    let mut s = fixture();
    let p = request(&s, "One", 1);
    s.execute("path.transform", p).unwrap();
    s.execute("edit.undo", json!({})).unwrap();
    for matrix in [
        json!([0, 0, 0, 1, 0, 0]),
        json!([-1, 0, 0, 1, 0, 0]),
        json!([1, 0.1, 0, 1, 0, 0]),
        json!([0, 1, -1, 0, 0, 0]),
        json!([1, 0, 0, 1, 1e100, 0]),
        json!([1, 0, 0, 1, null, 0]),
        json!([1, 0, 0]),
        json!(false),
    ] {
        let mut p = request(&s, "One", 1);
        p["matrix"] = matrix;
        reject(&mut s, p);
    }
    for key in ["layer", "quad", "warp", "style", "mesh", "rect", "angle", "scaleX"] {
        let mut p = request(&s, "One", 1);
        p[key] = json!(0);
        reject(&mut s, p);
    }
    for scale in [json!(0), json!(-2), json!(1e-15), json!(1e100), json!("2"), json!(null)] {
        let mut p = request(&s, "One", 1);
        p.as_object_mut().unwrap().remove("matrix");
        p["scaleX"] = scale;
        reject(&mut s, p);
    }
}
#[test]
fn component_transform_tests_stale_after_edit_rename_delete_and_undo_branch_is_rejected() {
    for action in ["edit", "rename", "delete", "undo"] {
        let mut s = fixture();
        let p = request(&s, "One", 1);
        match action {
            "edit" => {
                s.execute("path.transform", json!({"name":"Two","translateX":1})).unwrap();
            }
            "rename" => {
                s.execute("path.rename", json!({"name":"One","to":"Renamed"})).unwrap();
            }
            "delete" => {
                s.execute("path.delete", json!({"name":"One"})).unwrap();
            }
            _ => {
                s.execute("edit.undo", json!({})).unwrap();
            }
        }
        reject(&mut s, p);
    }
    let mut s = fixture();
    let p = request(&s, "One", 1);
    s.execute("path.transform", p.clone()).unwrap();
    s.execute("edit.undo", json!({})).unwrap();
    reject(&mut s, p);
    let p = request(&s, "One", 2);
    s.execute("path.transform", p).unwrap();
    assert_eq!(s.active().unwrap().history.redo_labels().count(), 0);
}
#[test]
fn component_transform_tests_noop_creates_no_history_and_text_geometry_stays_unchanged() {
    let mut s = fixture();
    s.execute("type.create", json!({"x":0,"y":0,"text":"Studio","size":16})).unwrap();
    s.execute("type.attachToPath", json!({})).unwrap();
    let layers = s.active().unwrap().doc.layers.clone();
    let history = s.active().unwrap().history.past_len();
    let revision = s.active().unwrap().revision;
    let mut p = request(&s, "work", 1);
    p["matrix"] = json!([1, 0, 0, 1, 0, 0]);
    s.execute("path.transform", p).unwrap();
    assert_eq!(s.active().unwrap().history.past_len(), history);
    assert_eq!(s.active().unwrap().revision, revision);
    for name in ["One", "work"] {
        let p = request(&s, name, 1);
        s.execute("path.transform", p).unwrap();
        assert_eq!(s.active().unwrap().doc.layers, layers);
    }
}
#[test]
fn component_transform_tests_whole_path_remains_legacy_when_selector_absent() {
    let mut s = fixture();
    for name in ["work", "One"] {
        let before = target(&s, name);
        s.execute("path.transform", json!({"name":name,"matrix":[2,0,0,3,4,5]})).unwrap();
        assert_eq!(target(&s, name), before.transform(&Affine { m: [2.0, 0.0, 0.0, 3.0, 4.0, 5.0] }));
    }
}

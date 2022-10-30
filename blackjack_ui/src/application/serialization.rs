// Copyright (C) 2022 setzer22 and contributors
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use crate::{
    graph::graph_interop::{self, NodeMapping},
    prelude::graph::*,
    prelude::*,
};
use std::path::{Path, PathBuf};

use blackjack_engine::{
    graph::{
        serialization::{RuntimeData, SerializedBjkGraph, SerializedUiData},
        BjkGraph, BjkNodeId, BlackjackValue, DependencyKind, InputParameter, NodeDefinitions,
    },
    graph_interpreter::{ExternalParameter, ExternalParameterValues},
};
use egui_node_graph::PanZoom;

pub fn save(
    editor_state: &GraphEditorState,
    custom_state: &CustomGraphState,
    path: impl AsRef<Path>,
) -> Result<()> {
    let (bjk_graph, mapping) =
        graph_interop::ui_graph_to_blackjack_graph(&editor_state.graph, custom_state)?;
    let external_param_values =
        graph_interop::extract_graph_params(&editor_state.graph, &bjk_graph, &mapping)?;
    let (mut serialized, id_map) =
        blackjack_engine::graph::serialization::SerializedBjkGraph::from_runtime(RuntimeData {
            graph: bjk_graph,
            external_parameters: Some(external_param_values),
        })?;

    let node_id_to_idx =
        |id: NodeId| -> usize { id_map.get_idx(mapping[id]).expect("Id should exist") };

    let node_positions = editor_state
        .node_positions
        .iter()
        .map(|(node_id, pos2)| (node_id_to_idx(node_id), glam::Vec2::new(pos2.x, pos2.y)))
        .sorted_by_key(|(idx, _pos)| *idx)
        .map(|(_idx, pos)| pos)
        .collect();

    let node_order = editor_state
        .node_order
        .iter_cpy()
        .map(node_id_to_idx)
        .collect();
    let pan = editor_state.pan_zoom.pan;

    serialized.set_ui_data(SerializedUiData {
        node_positions,
        node_order,
        active_node: custom_state.active_node.map(node_id_to_idx),
        pan: Vec2::new(pan.x, pan.y),
        zoom: editor_state.pan_zoom.zoom,
    });

    serialized.write_to_file(path)?;

    Ok(())
}

pub fn load(
    path: PathBuf,
    node_definitions: &NodeDefinitions,
) -> Result<(GraphEditorState, CustomGraphState)> {
    let serialized = SerializedBjkGraph::load_from_file(&path)?;
    let (runtime, ui_data, id_idx_mappings) = serialized.to_runtime()?;

    if ui_data.is_none() {
        bail!(
            "The file at {} doesn't have UI information. Cannot load.",
            path.to_string_lossy()
        )
    }
    let ui_data = ui_data.unwrap();

    let (graph, mapping) = graph_interop::blackjack_graph_to_ui_graph(
        &runtime.graph,
        &runtime.external_parameters,
        node_definitions,
    )?;
    let idx_to_node_id = |idx| mapping[id_idx_mappings.get_id(idx).expect("Should exist")];

    let node_order = ui_data
        .node_order
        .iter()
        .map(|idx| idx_to_node_id(*idx))
        .collect();

    let node_positions = ui_data
        .node_positions
        .iter()
        .enumerate()
        .map(|(idx, pos)| (idx_to_node_id(idx), egui::pos2(pos.x, pos.y)))
        .collect();

    let mut promoted_params = HashMap::default();
    for (bjk_node_id, bjk_node) in &runtime.graph.nodes {
        for bjk_input in &bjk_node.inputs {
            if let DependencyKind::External {
                promoted: Some(promoted),
            } = &bjk_input.kind
            {
                let input_id = graph[mapping[bjk_node_id]]
                    .get_input(&bjk_input.name)
                    .expect("Should exist");
                promoted_params.insert(input_id, promoted.clone());
            }
        }
    }

    let editor_state = GraphEditorState {
        graph,
        node_order,
        connection_in_progress: None,
        selected_node: None,
        node_positions,
        node_finder: None,
        pan_zoom: PanZoom {
            pan: egui::vec2(ui_data.pan.x, ui_data.pan.y),
            zoom: ui_data.zoom,
        },
        _user_state: std::marker::PhantomData,
    };
    let custom_state = CustomGraphState {
        run_side_effect: None,
        active_node: ui_data.active_node.map(idx_to_node_id),
        node_definitions: node_definitions.share(),
        promoted_params,
    };

    Ok((editor_state, custom_state))
}

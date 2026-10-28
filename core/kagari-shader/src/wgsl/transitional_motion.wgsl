// Transitional motion role:
// `fn effect(start: KgState, end: KgState, progress: f32, t: f32) -> KgState`
// (WGSL reserves `from`, so parameters cannot use that name).

@group(2) @binding(0) var<storage, read> args: Args;

@compute @workgroup_size(1)
fn kg_cs() {
    kg_write(effect(kg_state(kg_motion.kg_from), kg_state(kg_motion.kg_to), kg_motion.timing.y, kg_time()));
}

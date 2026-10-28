// Local motion role: `fn effect(state: KgState, t: f32) -> KgState`.

@group(2) @binding(0) var<storage, read> args: Args;

@compute @workgroup_size(1)
fn kg_cs() {
    var input = kg_state(kg_motion.pose);
    if kg_motion.chained != 0u {
        input = kg_state(kg_states[kg_motion.slot]);
    }
    kg_write(effect(input, kg_time()));
}

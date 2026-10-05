// Ordinary model lighting. No per-model image/sample/cache descriptors.
module model_mesh_lighting;
import tracer_types;
[[vk::binding(0,0)]] public ConstantBuffer<U_GuiInput> gui_input;
[[vk::binding(1,0)]] public ConstantBuffer<U_SunInfo> sun_info;
[[vk::binding(2,0)]] public ConstantBuffer<U_ShadingInfo> shading_info;
[[vk::binding(3,0)]] public ConstantBuffer<U_CameraInfo> camera_info;
[[vk::binding(4,0)]] public ConstantBuffer<U_ShadowCameraInfo> shadow_camera_info;

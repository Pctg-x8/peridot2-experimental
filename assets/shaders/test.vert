#version 450

layout(location = 0) in vec4 pos;
layout(location = 1) in vec4 col;
layout(location = 0) out vec4 ocol;
out gl_PerVertex { out vec4 gl_Position; };

layout(push_constant) uniform ScreenProperties {
    vec2 screenSize;
};

layout(set = 0, binding = 0) uniform ObjectTransform {
    mat4 objectTransform;
};

void main() {
    gl_Position = objectTransform * pos * vec4(1.0f / screenSize.x, -1.0f / screenSize.y, 1.0f, 1.0f);
    ocol = col;
}

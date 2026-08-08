#version 450

layout(location = 0) in vec2 qt_TexCoord0;
layout(location = 0) out vec4 fragColor;

layout(std140, binding = 0) uniform buf {
    mat4 qt_Matrix;
    float qt_Opacity;
    float eyeX;
    float eyeY;
    float distanceMm;
    float contentX;
    float contentY;
    float contentW;
    float contentH;
    float tiltDeg;
    float yawDeg;
    float mapX;
    float mapY;
    float mapW;
    float mapH;
    float srcX;
    float srcY;
    float srcW;
    float srcH;
    float sourceW;
    float sourceH;
    float letterR;
    float letterG;
    float letterB;
} ubuf;

layout(binding = 1) uniform sampler2D source;

const float PI = 3.14159265358979323846;

float degToRad(float deg) {
    return deg * (PI / 180.0);
}

vec3 rotateTiltYaw(vec3 p, float tiltDeg, float yawDeg) {
    float t = degToRad(tiltDeg);
    float y = degToRad(yawDeg);
    float ct = cos(t);
    float st = sin(t);
    float cy = cos(y);
    float sy = sin(y);
    // Rx(tilt)
    vec3 p1 = vec3(p.x, p.y * ct - p.z * st, p.y * st + p.z * ct);
    // Ry(yaw)
    return vec3(p1.x * cy + p1.z * sy, p1.y, -p1.x * sy + p1.z * cy);
}

void main() {
    float u = qt_TexCoord0.x;
    float v = qt_TexCoord0.y;
    float cx = ubuf.contentX + ubuf.contentW * 0.5;
    float cy = ubuf.contentY + ubuf.contentH * 0.5;
    float lx = (u - 0.5) * ubuf.contentW;
    float ly = (v - 0.5) * ubuf.contentH;
    vec3 local = rotateTiltYaw(vec3(lx, ly, 0.0), ubuf.tiltDeg, ubuf.yawDeg);
    vec3 P = vec3(cx + local.x, cy + local.y, local.z);
    vec3 E = vec3(ubuf.eyeX, ubuf.eyeY, -ubuf.distanceMm);
    vec3 D = P - E;
    float ax = atan(D.x, max(D.z, 1e-6));
    float ay = atan(D.y, max(D.z, 1e-6));

    // Map rectangle corners on z=0 through the same eye.
    vec2 mapCorners[4] = vec2[](
        vec2(ubuf.mapX, ubuf.mapY),
        vec2(ubuf.mapX + ubuf.mapW, ubuf.mapY),
        vec2(ubuf.mapX, ubuf.mapY + ubuf.mapH),
        vec2(ubuf.mapX + ubuf.mapW, ubuf.mapY + ubuf.mapH)
    );
    float aleft = 1e20;
    float aright = -1e20;
    float atop = 1e20;
    float abottom = -1e20;
    for (int i = 0; i < 4; ++i) {
        vec3 Dm = vec3(mapCorners[i].x - ubuf.eyeX, mapCorners[i].y - ubuf.eyeY, ubuf.distanceMm);
        float mx = atan(Dm.x, max(Dm.z, 1e-6));
        float my = atan(Dm.y, max(Dm.z, 1e-6));
        aleft = min(aleft, mx);
        aright = max(aright, mx);
        atop = min(atop, my);
        abottom = max(abottom, my);
    }

    float nu = (abs(aright - aleft) < 1e-9) ? 0.5 : (ax - aleft) / (aright - aleft);
    float nv = (abs(abottom - atop) < 1e-9) ? 0.5 : (ay - atop) / (abottom - atop);
    if (nu < 0.0 || nu > 1.0 || nv < 0.0 || nv > 1.0) {
        fragColor = vec4(ubuf.letterR, ubuf.letterG, ubuf.letterB, 1.0) * ubuf.qt_Opacity;
        return;
    }

    float sx = ubuf.srcX + nu * ubuf.srcW;
    float sy = ubuf.srcY + nv * ubuf.srcH;
    float texU = sx / max(ubuf.sourceW, 1.0);
    float texV = sy / max(ubuf.sourceH, 1.0);
    fragColor = texture(source, vec2(texU, texV)) * ubuf.qt_Opacity;
}

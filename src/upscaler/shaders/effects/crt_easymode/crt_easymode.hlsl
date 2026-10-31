// ============================================================================
//  CRT-Easymode (manual port from Magpie)
//  --------------------------------------
//  Source: Magpie src/Effects/CRT/CRT_Easymode.hlsl
//          (https://github.com/Blinue/Magpie, commit 241e0a44, GPL-3.0)
//  Originally ported by Magpie from
//          https://github.com/libretro/common-shaders/blob/master/crt/shaders/crt-easymode.cg
//
//  CRT Shader by EasyMode
//  License: GPL
//
//  A flat CRT shader ideally for 1080p or higher displays.
//
//  Example RGB Mask Parameter Settings:
//    Aperture Grille (Default): Dot Width 1, Dot Height 1, Stagger 0
//    Lottes' Shadow Mask:       Dot Width 2, Dot Height 1, Stagger 3
//
//  Port notes:
//    - Magpie runs this as a pixel-shader pass (//!STYLE PS) that also acts
//      as the scaler (OUTPUT has the window size). Here it is a 16x16 compute
//      shader writing an RGBA8 UAV sized to the destination rectangle.
//    - The input is BGRA (see ../bgra_input.hlsli); the result is swapped
//      back to BGRA when written.
//    - Magpie's implicit GetInput*/GetOutput* helpers are provided from the
//      constant buffer below.
//    - The body (helpers + Pass1) is unchanged.
//
//  Distributed under the GPL-3.0 license.
// ============================================================================

#include "../bgra_input.hlsli"
[[vk::image_format("rgba8")]]
RWTexture2D<float4> OUTPUT : register(u0);
SamplerState sam : register(s0); // POINT, CLAMP

cbuffer Constants : register(b0) {
  uint2 _inputSize;
  uint2 _outputSize;
  float2 _inputPt;
  float2 _outputPt;
  float sharpnessH;           // 0.5  [0, 1]
  float sharpnessV;           // 1    [0, 1]
  float maskStrength;         // 0.3  [0, 1]
  int maskDotWidth;           // 1    [1, 100]
  int maskDotHeight;          // 1    [1, 100]
  int maskStagger;            // 0    [0, 100]
  int maskSize;               // 1    [1, 100]
  float scanlineStrength;     // 1    [0, 1]
  float scanlineBeamWidthMin; // 1.5  [0.5, 5]
  float scanlineBeamWidthMax; // 1.5  [0.5, 5]
  float scanlineBrightMin;    // 0.35 [0, 1]
  float scanlineBrightMax;    // 0.65 [0, 1]
  int scanlineCutoff;         // 400  [1, 1000]
  float gammaInput;           // 2    [0.1, 5]
  float gammaOutput;          // 1.8  [0.1, 5]
  float brightBoost;          // 1.2  [1, 2]
  int dilation;               // 1    [0, 1]
  uint _pad0;
  uint _pad1;
  uint _pad2;
};

uint2 GetInputSize() { return _inputSize; }
float2 GetInputPt() { return _inputPt; }
uint2 GetOutputSize() { return _outputSize; }
float2 GetOutputPt() { return _outputPt; }

#pragma warning(disable: 3571) // X3571: pow(f, e) will not work for negative f, use abs(f) or conditionally handle negative values if you expect them

#define FIX(c) max(abs(c), 1e-5)
#define PI 3.141592653589
#define TEX2D(c) dilate(INPUT.SampleLevel(sam, c, 0))
#define mod(x,y) (x - y * trunc(x/y))

// Set to 0 to use linear filter and gain speed
#define ENABLE_LANCZOS 1

float4 dilate(float4 col) {
	float4 x = lerp(float4(1, 1, 1, 1), col, dilation);

	return col * x;
}

float curve_distance(float x, float sharp) {
	/*
		apply half-circle s-curve to distance for sharper (more pixelated) interpolation
		single line formula for Graph Toy:
		0.5 - sqrt(0.25 - (x - step(0.5, x)) * (x - step(0.5, x))) * sign(0.5 - x)
	*/

	float x_step = step(0.5, x);
	float curve = 0.5 - sqrt(0.25 - (x - x_step) * (x - x_step)) * sign(0.5 - x);

	return lerp(x, curve, sharp);
}

float4x4 get_color_matrix(float2 co) {
	float2 inputPt = GetInputPt();
	return float4x4(TEX2D(co - float2(inputPt.x, 0)), TEX2D(co), TEX2D(co + float2(inputPt.x, 0)), TEX2D(co + float2(2.0 * inputPt.x, 0)));
}

float3 filter_lanczos(float4 coeffs, float4x4 color_matrix) {
	float4 col = mul(coeffs, color_matrix);
	float4 sample_min = min(color_matrix[1], color_matrix[2]);
	float4 sample_max = max(color_matrix[1], color_matrix[2]);

	col = clamp(col, sample_min, sample_max);

	return col.rgb;
}

float4 Pass1(float2 pos) {
	float2 inputPt = GetInputPt();
	int2 inputSize = GetInputSize();
	int2 outputSize = GetOutputSize();
	float2 outputPt = GetOutputPt();

	float2 pix_co = pos * inputSize - 0.5f;
	
	float2 tex_co = (floor(pix_co) + 0.5) * inputPt;
	float2 dist = frac(pix_co);
	float curve_x;
	float3 col, col2;

#if ENABLE_LANCZOS
	curve_x = curve_distance(dist.x, sharpnessH * sharpnessH);

	float4 coeffs = PI * float4(1.0 + curve_x, curve_x, 1.0 - curve_x, 2.0 - curve_x);

	coeffs = FIX(coeffs);
	coeffs = 2.0 * sin(coeffs) * sin(coeffs / 2.0) / (coeffs * coeffs);
	coeffs /= dot(coeffs, 1.0);

	col = filter_lanczos(coeffs, get_color_matrix(tex_co));
	col2 = filter_lanczos(coeffs, get_color_matrix(tex_co + float2(0, inputPt.y)));
#else
	curve_x = curve_distance(dist.x, sharpnessH);

	col = lerp(TEX2D(tex_co).rgb, TEX2D(tex_co + float2(inputPt.x, 0)).rgb, curve_x);
	col2 = lerp(TEX2D(tex_co + float2(0, inputPt.y)).rgb, TEX2D(tex_co + float2(inputPt.x, inputPt.y)).rgb, curve_x);
#endif

	col = lerp(col, col2, curve_distance(dist.y, sharpnessV));
	col = pow(col, gammaInput / (dilation + 1.0));

	float luma = dot(float3(0.2126, 0.7152, 0.0722), col);
	float bright = (max(col.r, max(col.g, col.b)) + luma) / 2.0;
	float scan_bright = clamp(bright, scanlineBrightMin, scanlineBrightMax);
	float scan_beam = clamp(bright * scanlineBeamWidthMax, scanlineBeamWidthMin, scanlineBeamWidthMax);
	float scan_weight = 1.0 - pow(cos(pos.y * 2.0 * PI * inputSize.y) * 0.5 + 0.5, scan_beam) * scanlineStrength;

	float mask = 1.0 - maskStrength;
	float2 mod_fac = floor(pos * outputSize / float2(maskSize, maskDotHeight * maskSize));
	int dot_no = int(mod((mod_fac.x + mod(mod_fac.y, 2.0) * maskStagger) / maskDotWidth, 3.0));
	float3 mask_weight;

	if (dot_no == 0) mask_weight = float3(1.0, mask, mask);
	else if (dot_no == 1) mask_weight = float3(mask, 1.0, mask);
	else mask_weight = float3(mask, mask, 1.0);

	if (inputSize.y >= scanlineCutoff) scan_weight = 1.0;

	col2 = col.rgb;
	col *= scan_weight;
	col = lerp(col, col2, scan_bright);
	col *= mask_weight;
	col = pow(col, 1.0 / gammaOutput);

	return float4(col * brightBoost, 1);
}

// ----------------------------------------------------------------------------
//  Compute entry point (replaces Magpie's PS dispatch)
// ----------------------------------------------------------------------------
[numthreads(16, 16, 1)] void main(uint3 dtid : SV_DispatchThreadID) {
  if (dtid.x >= _outputSize.x || dtid.y >= _outputSize.y)
    return;

  float2 pos = (dtid.xy + 0.5f) * _outputPt;
  OUTPUT[dtid.xy] = Pass1(pos).bgra; // back to BGRA
}

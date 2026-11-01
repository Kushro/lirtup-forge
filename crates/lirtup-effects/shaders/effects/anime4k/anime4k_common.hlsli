// ============================================================================
//  Anime4K_Upscale_S - shared definitions for the four compute passes
//  --------------------------------------------------------------------------
//  Replaces what Magpie injects implicitly into every pass (StubDefs.hlsli):
//    - MF* types: Magpie maps them to min16float when //!CAPABILITY FP16 is
//      available. Here they are plain 32-bit floats (16-bit types would need
//      -enable-16bit-types and shaderFloat16, which the build does not use).
//    - Rmp8x8(), MulAdd(), GetInput*/GetOutput* helpers.
//    - The effect-wide sizes: "input" is the effect INPUT (also the size of
//      tex1/tex2), "output" is the 2x OUTPUT, for every pass.
//
//  Magpie is distributed under the GPL-3.0 license.
// ============================================================================

#define MP_BLOCK_SIZE 16

#define MF float
#define MF2 float2
#define MF3 float3
#define MF4 float4
#define MF3x4 float3x4
#define MF4x4 float4x4

cbuffer Constants : register(b0) {
  uint2 _inputSize;
  uint2 _outputSize;
  float2 _inputPt;
  float2 _outputPt;
};

uint2 GetInputSize() { return _inputSize; }
float2 GetInputPt() { return _inputPt; }
uint2 GetOutputSize() { return _outputSize; }
float2 GetOutputPt() { return _outputPt; }

// Map 64 threads onto an 8x8 grid (Magpie: Rmp8x8)
uint2 Rmp8x8(uint a) { return uint2(a / 8, a % 8); }

MF4 MulAdd(MF3 x, MF3x4 y, MF4 a) { return mul(x, y) + a; }
MF4 MulAdd(MF4 x, MF4x4 y, MF4 a) { return mul(x, y) + a; }

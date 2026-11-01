// Swaps the red and blue channels.
// The capture/upscale pipeline carries BGRA bytes in R8G8B8A8 textures, while
// MagpieFX effects expect RGBA. Normally the swap is folded into the
// generated shaders (see swizzle.py); this pass is the fallback for effects
// that use INPUT/OUTPUT in ways the rewriter cannot handle (e.g. FXAA passes
// INPUT to a function).

//!MAGPIE EFFECT
//!VERSION 4

//!TEXTURE
Texture2D INPUT;

//!TEXTURE
//!WIDTH INPUT_WIDTH
//!HEIGHT INPUT_HEIGHT
Texture2D OUTPUT;

//!PASS 1
//!STYLE PS
//!IN INPUT
//!OUT OUTPUT

float4 Pass1(float2 pos) {
	return INPUT.Load(int3(pos * GetInputSize(), 0)).bgra;
}

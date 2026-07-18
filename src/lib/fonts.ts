// Bundled, self-hosted fonts (SIL OFL 1.1). Latin -> Noto Sans, Korean -> Noto Sans KR.
// Imported here so Vite bundles the woff2 into dist for offline use (no external CDN).
// Only the subsets we ship (Latin + Korean, weights 400/700) are imported to keep the
// bundle small; Noto Sans covers Latin, Noto Sans KR covers Hangul.
import "@fontsource/noto-sans/latin-400.css";
import "@fontsource/noto-sans/latin-700.css";
import "@fontsource/noto-sans/latin-ext-400.css";
import "@fontsource/noto-sans/latin-ext-700.css";
import "@fontsource/noto-sans-kr/korean-400.css";
import "@fontsource/noto-sans-kr/korean-700.css";

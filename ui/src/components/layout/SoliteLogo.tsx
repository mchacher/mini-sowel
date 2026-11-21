// Derived from Sowel's SowelLogo.tsx, commit 5da84dc1 (Sowel spec 210): one drawing in two
// weights chosen by size, the same wink; plus Solite's antenna and the outlined wordmark.
import {
  HOUSE,
  MARK_RATIO,
  MASTER,
  SMALL,
  SMALL_BELOW_PX,
  VIEW_BOX,
  type LogoWeight,
} from "../../lib/logo";
import { WORDMARK_PATH, WORDMARK_RATIO, WORDMARK_VIEW_BOX } from "../../lib/wordmark";

interface SoliteLogoProps {
  /** The mark's width in px; its height follows. */
  size?: number;
  className?: string;
  /** Show the wordmark "solite" to the right of the mark */
  showWordmark?: boolean;
  /** Animate the face (wink) */
  animated?: boolean;
}

const INK = "var(--color-primary)";
const FACE = "var(--color-primary-mid)";

/** SVG native wink animation on a single eye */
function WinkAnimation() {
  return (
    <animateTransform
      attributeName="transform"
      additive="sum"
      type="scale"
      values="1 1; 1 1; 1 0.1; 1 1; 1 1"
      keyTimes="0; 0.92; 0.95; 0.98; 1"
      dur="4s"
      repeatCount="indefinite"
    />
  );
}

/** One drawing; the weight only changes the strokes and the placement of the face and antenna. */
function Mark({ weight, animated }: { weight: LogoWeight; animated: boolean }) {
  const { mast, ball, waves } = weight.antenna;
  return (
    <>
      <path d={mast.path} fill="none" stroke={INK} strokeWidth={mast.width} strokeLinecap="round" />
      <circle cx={ball.cx} cy={ball.cy} r={ball.r} fill={FACE} />
      {waves.paths.map((d) => (
        <path
          key={d}
          d={d}
          fill="none"
          stroke={FACE}
          strokeWidth={waves.width}
          strokeLinecap="round"
        />
      ))}
      <path
        d={HOUSE}
        fill="none"
        stroke={INK}
        strokeWidth={weight.house}
        strokeLinejoin="round"
        strokeLinecap="round"
      />
      <path
        d={weight.leftEye}
        fill="none"
        stroke={FACE}
        strokeWidth={weight.eye}
        strokeLinecap="round"
      />
      <g transform={`translate(${weight.wink.origin})`}>
        {animated && <WinkAnimation />}
        <path
          d={weight.wink.path}
          fill="none"
          stroke={FACE}
          strokeWidth={weight.eye}
          strokeLinecap="round"
        />
      </g>
      <path
        d={weight.smilePath}
        fill="none"
        stroke={FACE}
        strokeWidth={weight.smile}
        strokeLinecap="round"
      />
    </>
  );
}

export function SoliteLogo({
  size = 32,
  className = "",
  showWordmark = false,
  animated = false,
}: SoliteLogoProps) {
  // The wordmark is 0.4 of the mark's width tall: 14.4 px at 36 px, the height of an "l" in the
  // 20 px title text.
  const wordmarkHeight = size * 0.4;
  return (
    <span role="img" aria-label="Solite" className={`inline-flex items-center ${className}`}>
      <svg
        xmlns="http://www.w3.org/2000/svg"
        viewBox={VIEW_BOX}
        width={size}
        height={size / MARK_RATIO}
        fill="none"
        className="flex-shrink-0"
        aria-hidden="true"
      >
        <Mark weight={size < SMALL_BELOW_PX ? SMALL : MASTER} animated={animated} />
      </svg>
      {showWordmark && (
        <svg
          xmlns="http://www.w3.org/2000/svg"
          viewBox={WORDMARK_VIEW_BOX}
          width={wordmarkHeight * WORDMARK_RATIO}
          height={wordmarkHeight}
          fill={INK}
          className="ml-2.5 mt-1 flex-shrink-0"
          aria-hidden="true"
        >
          <path d={WORDMARK_PATH} />
        </svg>
      )}
    </span>
  );
}

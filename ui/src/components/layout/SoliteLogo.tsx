// Derived from Sowel's SowelLogo.tsx, commit 88727672: the same house, face and wink,
// plus three short rays above the roof for Solite's "light".

interface SoliteLogoProps {
  size?: number;
  className?: string;
  /** Animate the face (wink) */
  animated?: boolean;
}

const MONO = "var(--color-primary)";

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

export function SoliteLogo({ size = 32, className = "", animated = false }: SoliteLogoProps) {
  return (
    <svg
      xmlns="http://www.w3.org/2000/svg"
      viewBox="25 0 150 170"
      width={size}
      height={(size * 170) / 150}
      fill="none"
      className={className}
    >
      <path
        d="M100 30 L160 90 Q165 95 160 100 L160 150 Q160 158 152 158 L48 158 Q40 158 40 150 L40 100 Q35 95 40 90 Z"
        fill="none"
        stroke={MONO}
        strokeWidth="12"
        strokeLinejoin="round"
        strokeLinecap="round"
      />
      <path
        d="M75 115 Q100 140 125 115"
        fill="none"
        stroke={MONO}
        strokeWidth="9"
        strokeLinecap="round"
      />
      {/* Left eye — static */}
      <path
        d="M78 95 Q83 87 88 95"
        fill="none"
        stroke={MONO}
        strokeWidth="8"
        strokeLinecap="round"
      />
      {/* Right eye — winks */}
      <g transform="translate(117, 91)">
        {animated && <WinkAnimation />}
        <path d="M-5 4 Q0 -4 5 4" fill="none" stroke={MONO} strokeWidth="8" strokeLinecap="round" />
      </g>
      {/* Three rays above the roof: Solite's light */}
      <path d="M100 7 L100 17" fill="none" stroke={MONO} strokeWidth="8" strokeLinecap="round" />
      <path d="M79 14 L86 22" fill="none" stroke={MONO} strokeWidth="8" strokeLinecap="round" />
      <path d="M121 14 L114 22" fill="none" stroke={MONO} strokeWidth="8" strokeLinecap="round" />
    </svg>
  );
}

import type { SVGProps } from "react";

const base = (p: SVGProps<SVGSVGElement>) => ({
  width: 18,
  height: 18,
  viewBox: "0 0 24 24",
  fill: "currentColor",
  "aria-hidden": true,
  ...p,
});

export const Play = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base(p)}><path d="M8 5.5v13a1 1 0 0 0 1.5.86l10.5-6.5a1 1 0 0 0 0-1.72L9.5 4.64A1 1 0 0 0 8 5.5z" /></svg>
);
export const Pause = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base(p)}><rect x="6" y="4.5" width="4" height="15" rx="1.2" /><rect x="14" y="4.5" width="4" height="15" rx="1.2" /></svg>
);
export const Next = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base(p)}><path d="M5 6.2v11.6a.8.8 0 0 0 1.2.7L15 13v4.5a1 1 0 0 0 2 0v-11a1 1 0 0 0-2 0V11L6.2 5.5a.8.8 0 0 0-1.2.7z" /></svg>
);
export const Prev = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base(p)} style={{ transform: "scaleX(-1)" }}><path d="M5 6.2v11.6a.8.8 0 0 0 1.2.7L15 13v4.5a1 1 0 0 0 2 0v-11a1 1 0 0 0-2 0V11L6.2 5.5a.8.8 0 0 0-1.2.7z" /></svg>
);
export const Shuffle = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base(p)} fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
    <path d="M3 7h3.5c2 0 3.2 1 4.3 2.7l2.4 4.6c1 1.7 2.3 2.7 4.3 2.7H21M18 14l3 3-3 3M3 17h3.5c1.2 0 2.1-.4 2.9-1.1M14.6 8.1c.8-.7 1.7-1.1 2.9-1.1H21M18 4l3 3-3 3" />
  </svg>
);
export const Repeat = ({ one, ...p }: SVGProps<SVGSVGElement> & { one?: boolean }) => (
  <svg {...base(p)} fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
    <path d="M17 3l3 3-3 3M4 11V9a3 3 0 0 1 3-3h13M7 21l-3-3 3-3M20 13v2a3 3 0 0 1-3 3H4" />
    {one && <text x="12" y="14.6" fontSize="7" textAnchor="middle" fill="currentColor" stroke="none" fontWeight="700">1</text>}
  </svg>
);
export const Heart = ({ filled, ...p }: SVGProps<SVGSVGElement> & { filled?: boolean }) => (
  <svg {...base(p)} fill={filled ? "currentColor" : "none"} stroke="currentColor" strokeWidth="1.8" strokeLinejoin="round">
    <path d="M12 20s-7-4.4-7-10a4 4 0 0 1 7-2.6A4 4 0 0 1 19 10c0 5.6-7 10-7 10z" />
  </svg>
);
export const LyricsIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base(p)} fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round">
    <path d="M4 6h16M4 11h11M4 16h8" /><circle cx="18" cy="17" r="2.2" /><path d="M20.2 17V10" />
  </svg>
);
export const ImageIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base(p)} fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinejoin="round">
    <rect x="3.5" y="3.5" width="17" height="17" rx="3" /><circle cx="9" cy="9" r="1.6" /><path d="M20 15l-4.5-4.5L6 20" />
  </svg>
);
export const More = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base(p)}><circle cx="5.5" cy="12" r="1.7" /><circle cx="12" cy="12" r="1.7" /><circle cx="18.5" cy="12" r="1.7" /></svg>
);
export const Close = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base(p)} fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round"><path d="M6 6l12 12M18 6L6 18" /></svg>
);
export const Minus = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base(p)} fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round"><path d="M6 12h12" /></svg>
);
export const Pin = ({ off, ...p }: SVGProps<SVGSVGElement> & { off?: boolean }) => (
  <svg {...base(p)} fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round">
    <path d="M9 4h6l-1 5 3 3H7l3-3-1-5zM12 12v8" />
    {off && <path d="M4 4l16 16" />}
  </svg>
);
export const Note = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base(p)}><path d="M9 17.5a2.5 2.5 0 1 1-2-2.45V5.6a1 1 0 0 1 .76-.97l9-2.2A1 1 0 0 1 18 3.4v11.1a2.5 2.5 0 1 1-2-2.45V7.1l-7 1.7v8.7z" /></svg>
);

export const Volume = ({ level = 2, ...p }: SVGProps<SVGSVGElement> & { level?: 0 | 1 | 2 }) => (
  <svg {...base(p)}>
    <path d="M4 9.5v5a1 1 0 0 0 1 1h3l4.4 3.6a.8.8 0 0 0 1.3-.6V5.5a.8.8 0 0 0-1.3-.6L8 8.5H5a1 1 0 0 0-1 1z" />
    {level === 0 ? (
      <path d="M17 9.5l4.5 5m0-5l-4.5 5" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" fill="none" />
    ) : (
      <path d="M16.5 8.8a4.5 4.5 0 0 1 0 6.4" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" fill="none" />
    )}
    {level === 2 && <path d="M19 6.3a8 8 0 0 1 0 11.4" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" fill="none" />}
  </svg>
);
export const QueueIcon = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base(p)} fill="none" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round">
    <path d="M4 6h12M4 11h12M4 16h7" />
    <path d="M16 14.5v5.5" />
    <circle cx="14" cy="20" r="2" fill="currentColor" stroke="none" />
    <path d="M16 14.5l4-1.2" />
  </svg>
);
export const Search = (p: SVGProps<SVGSVGElement>) => (
  <svg {...base(p)} fill="none" stroke="currentColor" strokeWidth="1.9" strokeLinecap="round">
    <circle cx="10.5" cy="10.5" r="6" />
    <path d="M15 15l5 5" />
  </svg>
);

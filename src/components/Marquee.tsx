import { useLayoutEffect, useRef, useState } from "react";

/** Single-line text that scrolls back and forth when it overflows. */
export function Marquee({ text, className }: { text: string; className?: string }) {
  const outer = useRef<HTMLDivElement>(null);
  const inner = useRef<HTMLSpanElement>(null);
  const [shift, setShift] = useState(0);

  useLayoutEffect(() => {
    const measure = () => {
      if (!outer.current || !inner.current) return;
      const d = inner.current.scrollWidth - outer.current.clientWidth;
      setShift(d > 2 ? d + 8 : 0);
    };
    measure();
    const ro = new ResizeObserver(measure);
    if (outer.current) ro.observe(outer.current);
    return () => ro.disconnect();
  }, [text]);

  return (
    <div ref={outer} className={`marquee ${className ?? ""}`} title={text}>
      <span
        ref={inner}
        className={shift ? "marquee-run" : undefined}
        style={shift ? ({ "--shift": `-${shift}px`, "--dur": `${Math.max(6, shift / 18)}s` } as React.CSSProperties) : undefined}
      >
        {text}
      </span>
    </div>
  );
}

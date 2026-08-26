"use client";

import { useEffect, useRef } from "react";

export default function Odometer({ value, className = "" }: { value: number; className?: string }) {
  const strips = useRef<(HTMLSpanElement | null)[]>([]);
  const len = useRef(0);

  useEffect(() => {
    const v = Math.max(0, Math.floor(value || 0));
    const str = String(v);
    if (len.current !== str.length) {
      len.current = str.length;
      // let React paint the new columns first, then roll from zeros
      const id = requestAnimationFrame(() => {
        str.split("").forEach((ch, i) => {
          strips.current[i]?.style.setProperty("transform", `translateY(-${ch}em)`);
        });
      });
      return () => cancelAnimationFrame(id);
    }
    str.split("").forEach((ch, i) => {
      strips.current[i]?.style.setProperty("transform", `translateY(-${ch}em)`);
    });
  }, [value]);

  const digits = String(Math.max(0, Math.floor(value || 0))).length || 1;

  return (
    <span className={`inline-flex gap-0.5 font-mono text-2xl font-extrabold leading-none h-[1em] overflow-hidden ${className}`}>
      {Array.from({ length: digits }).map((_, i) => (
        <span key={i} className="od-col">
          <span
            ref={(el) => {
              strips.current[i] = el;
            }}
            className="od-strip"
          >
            {Array.from({ length: 10 }).map((_, d) => (
              <span key={d}>{d}</span>
            ))}
          </span>
        </span>
      ))}
    </span>
  );
}

import { useEffect, useRef } from "react";
import type { AuraInfo } from "./api";

interface Dot {
  x: number;
  y: number;
  speed: number;
  size: number;
  phase: number;
}

const COUNTS: Record<AuraInfo["effect"], number> = {
  motes: 16,
  snow: 46,
  rain: 22,
  fireflies: 7,
  fog: 5,
  leaves: 12,
  blossom: 16,
  starfall: 3,
};

function seeded(count: number): Dot[] {
  let n = 7;
  const next = () => {
    n = (n * 16807) % 2147483647;
    return (n % 10000) / 10000;
  };
  return Array.from({ length: count }, () => ({
    x: next(),
    y: next(),
    speed: 0.5 + next(),
    size: 0.6 + next(),
    phase: next() * Math.PI * 2,
  }));
}

function draw(c: CanvasRenderingContext2D, aura: AuraInfo, dots: Dot[], w: number, h: number, t: number) {
  c.clearRect(0, 0, w, h);
  const [r, g, b] = aura.color.map((v) => Math.round(v * 255));
  const paint = (alpha: number) => (c.fillStyle = `rgba(${r},${g},${b},${alpha})`);
  const wrap = (v: number) => v - Math.floor(v);
  for (const d of dots) {
    c.beginPath();
    if (aura.effect === "rain") {
      const y = wrap(d.y + t * 0.9 * d.speed) * h;
      const x = d.x * w;
      // Matches the pale core of the drops on the desktop.
      c.fillStyle = "rgba(176,200,238,0.8)";
      c.fillRect(x, y, 1, 7 + d.size * 3);
      continue;
    }
    if (aura.effect === "starfall") {
      const k = wrap(d.y + t * 0.22 * d.speed);
      const x = (d.x * 0.7 + k * 0.4) * w;
      const y = k * h;
      c.strokeStyle = `rgba(${r},${g},${b},${0.9 * (1 - k)})`;
      c.lineWidth = 1.4;
      c.moveTo(x, y);
      c.lineTo(x - 16, y - 20);
      c.stroke();
      continue;
    }
    if (aura.effect === "leaves" || aura.effect === "blossom") {
      const leaf = aura.effect === "leaves";
      const y = wrap(d.y + t * (leaf ? 0.09 : 0.07) * d.speed) * h;
      const x = wrap(d.x + Math.sin(t * 0.8 + d.phase) * 0.05) * w;
      paint(0.95);
      c.ellipse(x, y, (leaf ? 4 : 2.8) * d.size, (leaf ? 1.8 : 2) * d.size, t + d.phase, 0, Math.PI * 2);
    } else if (aura.effect === "snow") {
      // Depth layers as on the desktop: small distant flakes, plump mid flakes,
      // and large soft foreground flakes swaying smoothly.
      const depth = (d.size - 0.6) ** 1.55;
      const y = wrap(d.y + t * (0.11 + 0.32 * depth) * d.speed) * h;
      const x = wrap(d.x + Math.sin(t * 0.7 + d.phase) * (0.015 + 0.045 * depth) + t * 0.01 * depth) * w;
      if (depth > 0.85) {
        paint(0.32);
        c.arc(x, y, 5.8, 0, Math.PI * 2);
      } else {
        paint(0.45 + 0.52 * depth);
        c.arc(x, y, 0.7 + depth * 1.6 + depth * depth * 2.2, 0, Math.PI * 2);
      }
    } else if (aura.effect === "motes") {
      const y = wrap(d.y - t * 0.05 * d.speed) * h;
      const x = wrap(d.x + Math.sin(t * 0.4 + d.phase) * 0.02) * w;
      paint(0.35 + 0.55 * Math.abs(Math.sin(t * 0.9 + d.phase)));
      c.arc(x, y, d.size * 1.1, 0, Math.PI * 2);
    } else if (aura.effect === "fireflies") {
      const x = (0.5 + (d.x - 0.5) * 0.9 + Math.sin(t * 0.5 + d.phase) * 0.05) * w;
      const y = (0.55 + (d.y - 0.5) * 0.8 + Math.cos(t * 0.4 + d.phase) * 0.06) * h;
      paint(0.15 + 0.85 * Math.max(0, Math.sin(t * 0.9 + d.phase)) ** 3);
      c.arc(x, y, 1.5 + d.size, 0, Math.PI * 2);
    } else {
      const x = wrap(d.x + t * 0.012 * d.speed) * w * 1.3 - w * 0.15;
      paint(0.14);
      c.ellipse(x, h * (0.72 + d.y * 0.25), w * 0.34, h * 0.14, 0, 0, Math.PI * 2);
    }
    c.fill();
  }
  if (aura.effect === "snow") {
    // Deep, rolling snow drift along the bottom with high corner mounds.
    c.beginPath();
    c.moveTo(0, h);
    for (let i = 0; i <= 24; i++) {
      const u = i / 24;
      const drift = 1 + 0.9 * Math.exp(-u * 6.5) + 0.9 * Math.exp(-(1 - u) * 6.5);
      c.lineTo(u * w, h - h * 0.1 * drift * (0.84 + 0.16 * Math.sin(u * 14)));
    }
    c.lineTo(w, h);
    c.closePath();
    paint(0.95);
    c.fill();
  }
}

export function AuraPreview({ aura, active }: { aura: AuraInfo; active: boolean }) {
  const canvas = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const element = canvas.current;
    const context = element?.getContext("2d");
    if (!element || !context) return;
    const scale = window.devicePixelRatio || 1;
    const { width, height } = element.getBoundingClientRect();
    element.width = Math.round(width * scale);
    element.height = Math.round(height * scale);
    context.scale(scale, scale);
    const dots = seeded(COUNTS[aura.effect]);
    const still = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
    draw(context, aura, dots, width, height, 2);
    if (!active || still) return;
    let frame = 0;
    let last = 0;
    const started = performance.now();
    const loop = (now: number) => {
      frame = requestAnimationFrame(loop);
      if (now - last < 33 || document.hidden) return;
      last = now;
      draw(context, aura, dots, width, height, 2 + (now - started) / 1000);
    };
    frame = requestAnimationFrame(loop);
    return () => cancelAnimationFrame(frame);
  }, [aura, active]);

  return <canvas ref={canvas} aria-hidden="true" className="block h-full w-full" />;
}

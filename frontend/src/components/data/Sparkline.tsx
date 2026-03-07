"use client";

import { useMemo } from "react";

interface SparklineProps {
  data: number[];
  width?: number;
  height?: number;
  color?: string;
  fillOpacity?: number;
  strokeWidth?: number;
  className?: string;
}

export default function Sparkline({
  data,
  width = 80,
  height = 24,
  color = "hsl(var(--primary))",
  fillOpacity = 0.1,
  strokeWidth = 1.5,
  className,
}: SparklineProps) {
  const path = useMemo(() => {
    if (data.length < 2) return "";
    const min = Math.min(...data);
    const max = Math.max(...data);
    const range = max - min || 1;
    const step = width / (data.length - 1);
    const points = data.map((v, i) => ({
      x: i * step,
      y: height - ((v - min) / range) * (height - 2) - 1,
    }));
    const line = points
      .map((p, i) => (i === 0 ? `M${p.x},${p.y}` : `L${p.x},${p.y}`))
      .join(" ");
    const fill = `${line} L${width},${height} L0,${height} Z`;
    return { line, fill };
  }, [data, width, height]);

  if (!path || data.length < 2) return null;

  return (
    <svg
      width={width}
      height={height}
      className={className}
      viewBox={`0 0 ${width} ${height}`}
    >
      <path d={path.fill} fill={color} opacity={fillOpacity} />
      <path
        d={path.line}
        fill="none"
        stroke={color}
        strokeWidth={strokeWidth}
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

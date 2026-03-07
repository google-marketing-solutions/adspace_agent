"use client";

import {
  ResponsiveContainer,
  AreaChart,
  Area,
  XAxis,
  YAxis,
  CartesianGrid,
  Tooltip,
} from "recharts";

interface DataPoint {
  name: string;
  [key: string]: string | number;
}

interface PerformanceChartProps {
  data: DataPoint[];
  dataKeys: { key: string; color: string; label?: string }[];
  height?: number;
  title?: string;
}

export default function PerformanceChart({
  data,
  dataKeys,
  height = 300,
  title,
}: PerformanceChartProps) {
  return (
    <div className="bg-white rounded-xl border border-gray-200 p-5 shadow-sm">
      {title && (
        <h3 className="text-sm font-semibold text-gray-700 mb-4">{title}</h3>
      )}
      <ResponsiveContainer width="100%" height={height}>
        <AreaChart data={data}>
          <defs>
            {dataKeys.map(({ key, color }) => (
              <linearGradient
                key={key}
                id={`grad-${key}`}
                x1="0"
                y1="0"
                x2="0"
                y2="1"
              >
                <stop offset="5%" stopColor={color} stopOpacity={0.3} />
                <stop offset="95%" stopColor={color} stopOpacity={0} />
              </linearGradient>
            ))}
          </defs>
          <CartesianGrid strokeDasharray="3 3" stroke="#f0f0f0" />
          <XAxis
            dataKey="name"
            tick={{ fontSize: 12 }}
            stroke="#9ca3af"
          />
          <YAxis tick={{ fontSize: 12 }} stroke="#9ca3af" />
          <Tooltip
            contentStyle={{
              borderRadius: "8px",
              border: "1px solid #e5e7eb",
              fontSize: "13px",
            }}
          />
          {dataKeys.map(({ key, color, label }) => (
            <Area
              key={key}
              type="monotone"
              dataKey={key}
              name={label ?? key}
              stroke={color}
              fill={`url(#grad-${key})`}
              strokeWidth={2}
            />
          ))}
        </AreaChart>
      </ResponsiveContainer>
    </div>
  );
}

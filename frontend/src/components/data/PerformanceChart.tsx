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
import { Card, CardContent, CardHeader, CardTitle } from "@/components/ui/card";

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
    <Card>
      {title && (
        <CardHeader className="pb-0">
          <CardTitle className="text-sm font-semibold">{title}</CardTitle>
        </CardHeader>
      )}
      <CardContent className={title ? "pt-4" : ""}>
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
            <CartesianGrid
              strokeDasharray="3 3"
              className="stroke-border/50"
            />
            <XAxis
              dataKey="name"
              tick={{ fontSize: 12 }}
              className="text-muted-foreground"
              stroke="hsl(var(--muted-foreground))"
            />
            <YAxis
              tick={{ fontSize: 12 }}
              className="text-muted-foreground"
              stroke="hsl(var(--muted-foreground))"
            />
            <Tooltip
              contentStyle={{
                borderRadius: "8px",
                border: "1px solid hsl(var(--border))",
                backgroundColor: "hsl(var(--card))",
                color: "hsl(var(--card-foreground))",
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
      </CardContent>
    </Card>
  );
}

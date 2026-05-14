import type { Config } from "tailwindcss";

export default {
  content: ["./index.html", "./settings.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        // Status dots are the only saturated color in the UI.
        dot: {
          idle: "#71717a",
          work: "#60a5fa",
          done: "#34d399",
          err: "#f87171",
        },
      },
      fontFamily: {
        sans: [
          "Segoe UI Variable",
          "Segoe UI",
          "system-ui",
          "-apple-system",
          "sans-serif",
        ],
      },
      fontSize: {
        label: ["12px", { lineHeight: "1" }],
        value: ["13px", { lineHeight: "1" }],
      },
      borderRadius: {
        surface: "14px",
      },
      transitionTimingFunction: {
        out: "cubic-bezier(0.16, 1, 0.3, 1)",
      },
      transitionDuration: {
        220: "220ms",
      },
      keyframes: {
        // 1.6s breathing pulse for working dot.
        breathe: {
          "0%, 100%": { opacity: "1", transform: "scale(1)" },
          "50%": { opacity: "0.6", transform: "scale(1.15)" },
        },
        // 400ms green wash on done.
        doneWash: {
          "0%": { backgroundColor: "rgba(52, 211, 153, 0.18)" },
          "100%": { backgroundColor: "rgba(52, 211, 153, 0.04)" },
        },
      },
      animation: {
        breathe: "breathe 1.6s ease-in-out infinite",
        "done-wash": "doneWash 400ms ease-out forwards",
      },
    },
  },
  plugins: [],
} satisfies Config;

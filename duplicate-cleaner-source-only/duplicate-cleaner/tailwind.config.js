/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  darkMode: "class",
  theme: {
    extend: {
      colors: {
        "surface-0": "#0c0d0f",
        "surface-1": "#121316",
        "surface-2": "#181a1e",
        "surface-3": "#1f2126",
        "surface-4": "#272a30",
        line: "#2a2d34",
        "line-soft": "rgba(255, 255, 255, 0.06)",
        accent: {
          DEFAULT: "#4f8cff",
          strong: "#2f6fe4",
          soft: "rgba(79, 140, 255, 0.14)",
        },
        violet: { DEFAULT: "#8b6cff", soft: "rgba(139, 108, 255, 0.14)" },
        cyan: { DEFAULT: "#38c7f5", soft: "rgba(56, 199, 245, 0.14)" },
        "text-primary": "#f2f3f5",
        "text-secondary": "#a7abb3",
        "text-tertiary": "#6b7079",
        success: { DEFAULT: "#3ecf8e", soft: "rgba(62, 207, 142, 0.14)" },
        warning: { DEFAULT: "#f5b533", soft: "rgba(245, 181, 51, 0.14)" },
        danger: { DEFAULT: "#f56c6c", soft: "rgba(245, 108, 108, 0.14)" },
      },
      borderRadius: {
        xl2: "18px",
      },
      fontFamily: {
        sans: [
          "Segoe UI Variable Display",
          "Segoe UI",
          "system-ui",
          "-apple-system",
          "Inter",
          "Helvetica Neue",
          "Arial",
          "sans-serif",
        ],
        mono: ["Cascadia Code", "SF Mono", "Consolas", "ui-monospace", "monospace"],
      },
      spacing: {
        4.5: "1.125rem",
        9.5: "2.375rem",
      },
      boxShadow: {
        card: "0 10px 30px -18px rgba(0, 0, 0, 0.7)",
      },
      keyframes: {
        "fade-in": {
          from: { opacity: "0", transform: "translateY(6px)" },
          to: { opacity: "1", transform: "translateY(0)" },
        },
        "slide-up": {
          from: { opacity: "0", transform: "translateY(14px) scale(0.985)" },
          to: { opacity: "1", transform: "translateY(0) scale(1)" },
        },
        "pulse-soft": {
          "0%, 100%": { opacity: "1" },
          "50%": { opacity: "0.55" },
        },
        shimmer: {
          "0%": { backgroundPosition: "-400px 0" },
          "100%": { backgroundPosition: "400px 0" },
        },
        indeterminate: {
          "0%": { transform: "translateX(-100%) scaleX(0.4)" },
          "50%": { transform: "translateX(20%) scaleX(0.6)" },
          "100%": { transform: "translateX(110%) scaleX(0.4)" },
        },
      },
      animation: {
        "fade-in": "fade-in 220ms cubic-bezier(0.2, 0.8, 0.2, 1) both",
        "slide-up": "slide-up 260ms cubic-bezier(0.2, 0.8, 0.2, 1) both",
        "pulse-soft": "pulse-soft 1.6s ease-in-out infinite",
        shimmer: "shimmer 1.6s linear infinite",
        indeterminate: "indeterminate 1.4s ease-in-out infinite",
      },
    },
  },
  plugins: [],
};

/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        // 设计图配色：深色卡片 + 紫色主色
        bg: {
          base: "#111111",
          panel: "#171717",
          card: "#202020",
          hover: "#2a2a2a",
        },
        brand: {
          DEFAULT: "#6c5ce7",
          hover: "#5a4ad7",
          soft: "rgba(108, 92, 231, 0.15)",
        },
        accent: {
          green: "#22c55e",
          red: "#ef4444",
          amber: "#f59e0b",
          blue: "#3b82f6",
        },
        ink: {
          primary: "#e8eaf0",
          secondary: "#c0c6d4",
          muted: "#8b94a8",
        },
        border: {
          subtle: "#303746",
          strong: "#465064",
        },
      },
      fontFamily: {
        sans: [
          "-apple-system",
          "BlinkMacSystemFont",
          "PingFang SC",
          "Microsoft YaHei",
          "Segoe UI",
          "sans-serif",
        ],
        mono: ["JetBrains Mono", "Consolas", "Menlo", "monospace"],
      },
    },
  },
  plugins: [],
};

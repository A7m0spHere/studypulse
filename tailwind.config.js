/** @type {import('tailwindcss').Config} */
export default {
  content: ["./index.html", "./src/**/*.{ts,tsx}"],
  theme: {
    extend: {
      colors: {
        ink: "#1f2933",
        paper: "#f7f4ee",
        tomato: "#d94c3d",
        moss: "#2f6f5e",
        line: "#ded8ce",
      },
      fontFamily: {
        sans: ["Aptos", "Segoe UI", "system-ui", "sans-serif"],
        mono: ["Cascadia Mono", "Consolas", "monospace"],
      },
      boxShadow: {
        panel: "0 10px 30px rgba(39, 36, 30, 0.08)",
      },
    },
  },
  plugins: [],
};

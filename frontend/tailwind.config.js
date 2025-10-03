/** @type {import('tailwindcss').Config} */
module.exports = {
  content: [
    "./src/pages/**/*.{js,ts,jsx,tsx,mdx}",
    "./src/components/**/*.{js,ts,jsx,tsx,mdx}",
    "./src/app/**/*.{js,ts,jsx,tsx,mdx}",
  ],
  theme: {
    extend: {
      colors: {
        primary: '#3B82F6',
        secondary: '#6B7280',
        markdown: {
          background: '#0d1117',
          card: '#161b22',
          block: '#30363d',
          inline: '#1f242c',
          border: '#30363d',
          text: '#c9d1d9',
          muted: '#8b949e',
          accent: '#58a6ff',
          'accent-hover': '#79c0ff',
          success: '#3fb950',
          warning: '#d29922',
          info: '#58a6ff',
          'callout-border': '#58a6ff66',
          'callout-bg': '#1f6feb1a',
        },
      },
      fontFamily: {
        'sans': ['var(--font-inter)', 'Inter', 'sans-serif'],
        'source-code': ['var(--font-source-code)', 'Source Code Pro', 'monospace'],
        'dot-gothic': ['var(--font-dot-gothic)', 'sans-serif'],
        'sometype': ['var(--font-sometype)', 'monospace'],
        'pixelify': ['var(--font-pixelify)', 'sans-serif'],
        'bauhaus': ['Bauhaus_Chez_Display_2.0', 'sans-serif'],
      },
      fontSize: {
        'xs-sm': ['13px', { lineHeight: '18px' }],
        'sm-base': ['15px', { lineHeight: '22px' }],
      }
    },
  },
  plugins: [],
}

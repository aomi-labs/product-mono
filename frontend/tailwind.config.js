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
      },
      fontFamily: {
        'sans': ['Inter', 'sans-serif'],
        'source-code': ['Source Code Pro', 'monospace'],
      },
      fontSize: {
        'xs-sm': ['13px', { lineHeight: '18px' }],
        'sm-base': ['15px', { lineHeight: '22px' }],
      }
    },
  },
  plugins: [],
}
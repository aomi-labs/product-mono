/** @type {import('tailwindcss').Config} */
export default {
  content: [
    "./index.html",
    "./src/**/*.{js,ts,jsx,tsx}",
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
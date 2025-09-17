# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Development Commands

- `npm run dev` - Start development server on port 3000 with auto-reload
- `npm run build` - Build for production (outputs to `dist/`)
- `npm run preview` - Preview production build on port 3000
- `npm run watch:css` - Watch and compile Tailwind CSS (outputs to `src/output.css`)

## Architecture

This is a single-page application (SPA) for Aomi Labs with client-side routing implemented in vanilla JavaScript. The application uses Vite as the build tool and Tailwind CSS for styling.

### Core Structure
- **Entry Point**: `index.html` loads the main JavaScript module
- **Router**: `src/main.js` contains a simple client-side router that handles `/` (landing) and `/blog` routes
- **Styling**: `src/input.css` contains Tailwind directives and custom animations
- **Assets**: Custom Bauhaus fonts and images stored in `assets/` directory

### Key Features
- **Client-side Routing**: Uses `window.history.pushState()` for navigation without page reloads
- **Scroll Animations**: Custom CSS classes (`.scroll-reveal`, `.slide-in-right`) with IntersectionObserver for reveal animations
- **Two Page Views**: Landing page with company information and blog page with rotating record visual
- **Custom Typography**: Bauhaus Chez Display 2.0 fonts loaded via `@font-face` declarations

### Page Components
The application renders different HTML templates based on the current route:
- `getLandingPageHTML()` - Main marketing page with features, mission, and contact sections
- `getBlogPageHTML()` - Blog/thoughts page with rotating record animation on scroll

### Animation System
- Uses IntersectionObserver for scroll-triggered animations
- Staggered reveal effects with CSS transition delays
- Blog page features a rotating record image that spins based on scroll position

### External Links
- GitHub integration with links to `https://github.com/aomi-labs`
- Notion blog posts linked from the thoughts/blog section
- Social media links (X/Twitter)
# Chatbot Frontend

A modern, responsive TypeScript frontend for a chatbot landing page built with Vite.

## Features

- 🚀 **Fast Development** - Vite for lightning-fast builds and HMR
- 📱 **Responsive Design** - Mobile-first approach with CSS Grid/Flexbox
- 🎯 **TypeScript** - Full type safety and modern JavaScript features
- 🧩 **Component Architecture** - Modular, reusable components
- 🎨 **Modern UI** - Clean, professional design with smooth animations
- 🔧 **Developer Tools** - ESLint for code quality and consistency

## Project Structure

```
frontend/
├── public/
│   └── favicon.svg           # Custom chatbot icon
├── src/
│   ├── components/           # UI components
│   │   ├── Header.ts         # Navigation header
│   │   ├── Hero.ts           # Landing page hero section
│   │   ├── Features.ts       # Features showcase grid
│   │   └── Footer.ts         # Site footer
│   ├── styles/
│   │   └── main.css          # Global styles and design system
│   ├── types/
│   │   └── index.ts          # TypeScript interfaces
│   ├── utils/
│   │   └── api.ts            # API utilities with axios
│   └── main.ts               # Application entry point
├── index.html                # Main HTML template
├── package.json              # Dependencies and scripts
├── tsconfig.json            # TypeScript configuration
├── vite.config.ts           # Vite build configuration
└── .eslintrc.json           # ESLint rules
```

## Getting Started

### Prerequisites

- Node.js 18+ 
- npm or yarn

### Installation

1. Navigate to the frontend directory:
```bash
cd frontend
```

2. Install dependencies:
```bash
npm install
```

3. Start the development server:
```bash
npm run dev
```

The application will be available at `http://localhost:3000`

## Available Scripts

- `npm run dev` - Start development server with hot reload
- `npm run build` - Build for production
- `npm run preview` - Preview production build locally
- `npm run lint` - Run ESLint for code quality checks
- `npm run type-check` - Run TypeScript type checking

## Design System

The project uses a consistent design system with CSS custom properties:

- **Primary Color**: `#6366f1` (Indigo)
- **Typography**: System fonts (-apple-system, BlinkMacSystemFont, Segoe UI)
- **Spacing**: Consistent rem-based spacing
- **Responsive**: Mobile-first breakpoints

## API Integration

The frontend is ready to connect to your chatbot backend:

- API utilities in `src/utils/api.ts`
- Configurable base URL (dev: `localhost:8080`, prod: `/api`)
- TypeScript interfaces for API responses
- Error handling and timeout configuration

## Components

### Header
- Responsive navigation
- Logo and menu links
- Sticky positioning

### Hero
- Eye-catching headline
- Call-to-action buttons
- Gradient background

### Features
- 6-item feature grid
- Hover animations
- Icon + text layout

### Footer
- Copyright and branding
- Clean, minimal design

## Customization

To customize the chatbot branding:

1. Update `index.html` title and meta description
2. Replace `/public/favicon.svg` with your icon
3. Modify CSS variables in `src/styles/main.css`
4. Update component text content
5. Configure API endpoints in `src/utils/api.ts`

## Browser Support

- Chrome 90+
- Firefox 88+
- Safari 14+
- Edge 90+

## License

MIT License - feel free to use this template for your chatbot projects!
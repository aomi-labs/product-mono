# Aomi Landing Page

A modern landing page for Aomi Labs with integrated blog functionality.

## Features

- **Landing Page**: Main marketing page with product information
- **Blog Integration**: Integrated blog section accessible at `/blog`
- **Client-side Routing**: Seamless navigation between pages
- **Responsive Design**: Built with Tailwind CSS
- **Custom Animations**: Scroll reveal and slide-in animations

## Development

### Prerequisites
- Node.js (version 16 or higher)
- npm

### Installation

1. Install dependencies:
```bash
npm install
```

2. Start the development server:
```bash
npm run dev
```

3. Open your browser and navigate to `http://localhost:3000`

### Available Routes

- `/` - Main landing page
- `/blog` - Blog/thoughts page

### Navigation

- Click the "Blog ↗" button on the landing page to navigate to the blog
- Click the Aomi logo on the blog page to return to the landing page
- Use browser back/forward buttons for navigation

## Project Structure

```
aomi-landing/
├── index.html              # Main HTML file
├── src/
│   ├── main.js            # JavaScript with routing logic
│   └── input.css          # Tailwind CSS with custom styles
├── assets/
│   ├── images/            # Project images and logos
│   └── fonts/             # Custom Bauhaus fonts
├── package.json           # Project dependencies
├── vite.config.js         # Vite configuration
└── tailwind.config.js     # Tailwind CSS configuration
```

## Build

Build for production:
```bash
npm run build
```

Preview production build:
```bash
npm run preview
```

## Technologies Used

- **Vite** - Fast development server and build tool
- **Tailwind CSS** - Utility-first CSS framework
- **Vanilla JavaScript** - Client-side routing and interactions
- **Custom Fonts** - Bauhaus Chez Display 2.0 typography 
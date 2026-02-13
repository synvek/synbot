# Synbot Documentation (VitePress)

This directory contains the Synbot documentation built with [VitePress](https://vitepress.dev/).

## Project Structure

```
docs/
├── .vitepress/              # VitePress configuration
│   ├── config.js           # Site configuration
│   ├── theme/              # Custom theme
│   │   ├── index.js       # Theme entry point
│   │   ├── styles/        # Custom styles
│   │   ├── components/    # Vue components
│   │   └── scripts/       # JavaScript files
│   └── dist/              # Built site (generated)
├── en/                     # English documentation
│   ├── getting-started/   # Installation, configuration, etc.
│   ├── user-guide/        # User guides
│   ├── developer-guide/   # Developer documentation
│   ├── examples/          # Examples
│   └── index.md           # English homepage
├── zh/                     # Chinese documentation
│   └── ...                # Same structure as en/
├── index.md               # Main landing page
├── package.json           # Node.js dependencies
└── README.md              # This file
```

## Development

### Prerequisites

- Node.js 18 or higher
- npm or yarn

### Installation

```bash
# Install dependencies
npm install

# Or with yarn
yarn install
```

### Local Development

```bash
# Start development server
npm run dev

# The site will be available at http://localhost:5173/docs/
```

### Build for Production

```bash
# Build static site
npm run build

# Preview built site
npm run preview
```

## Features

### Bilingual Support
- Complete documentation in English and Chinese
- Language switcher component
- Separate directories for each language

### Dark/Light Mode
- Built-in theme switching
- Respects system preferences
- Smooth transitions

### Responsive Design
- Mobile-friendly layout
- Accessible navigation
- Optimized for all screen sizes

### Search
- Full-text search
- Real-time results
- Keyboard navigation

### Code Examples
- Syntax highlighting
- Copy-to-clipboard buttons
- Line numbers

## Deployment

### GitHub Pages

```bash
# Build and deploy to GitHub Pages
npm run deploy
```

### Netlify

1. Connect your repository to Netlify
2. Set build command: `npm run build`
3. Set publish directory: `.vitepress/dist`
4. Deploy!

### Vercel

1. Import your repository to Vercel
2. VitePress is automatically detected
3. Deploy!

### Manual Deployment

```bash
# Build the site
npm run build

# The built site is in .vitepress/dist/
# Upload this directory to any static hosting service
```

## Adding Content

### Create a New Page

1. Create a new Markdown file in the appropriate language directory
2. Add frontmatter:

```yaml
---
title: Page Title
description: Page description
---
```

3. Write content using Markdown with VitePress extensions

### Frontmatter Options

- `title`: Page title
- `description`: Page description for SEO
- `lang`: Language code (en, zh)
- `layout`: Page layout (doc, home, page)

### Markdown Extensions

VitePress supports all standard Markdown plus:

- Custom containers: `::: info`, `::: warning`, `::: danger`
- Code groups and tabs
- Relative links
- Emoji: `:smile:`
- Custom components

## Customization

### Theme

Edit `.vitepress/theme/` files:

- `styles/custom.css`: Custom styles
- `components/`: Vue components
- `index.js`: Theme configuration

### Configuration

Edit `.vitepress/config.js` for:

- Site metadata
- Navigation and sidebar
- Theme colors
- Plugin configuration

## Performance Optimization

### Build Optimization

```bash
# Production build with minification
npm run build
```

### Image Optimization

- Use optimized image formats (WebP, AVIF)
- Compress images before adding
- Use appropriate sizes

### Code Splitting

VitePress automatically code-splits pages for optimal loading.

## Troubleshooting

### Common Issues

#### 1. Build Errors
```bash
# Clear node_modules and reinstall
rm -rf node_modules package-lock.json
npm install
```

#### 2. Missing Dependencies
```bash
# Install missing dependencies
npm install vitepress vue
```

#### 3. Port Already in Use
```bash
# Use a different port
npm run dev -- --port 5174
```

#### 4. Search Not Working
- Ensure `search: { provider: 'local' }` is in config
- Rebuild the site: `npm run build`

### Debugging

```bash
# Enable debug mode
DEBUG=vitepress:* npm run dev
```

## Contributing

1. Fork the repository
2. Create a feature branch
3. Make changes
4. Test locally: `npm run dev`
5. Submit a pull request

## License

MIT License - see [LICENSE](../LICENSE) file.
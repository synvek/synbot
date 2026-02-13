// Theme switching functionality
class ThemeManager {
  constructor() {
    this.themeToggle = document.getElementById('theme-toggle');
    this.htmlElement = document.documentElement;
    this.themeKey = 'synbot-theme';
    this.init();
  }

  init() {
    // Load saved theme or default to light
    const savedTheme = localStorage.getItem(this.themeKey) || 'light';
    this.setTheme(savedTheme);

    // Add event listener to toggle button
    if (this.themeToggle) {
      this.themeToggle.addEventListener('click', () => this.toggleTheme());
    }

    // Listen for system theme changes
    this.watchSystemTheme();
  }

  setTheme(theme) {
    this.htmlElement.setAttribute('data-theme', theme);
    localStorage.setItem(this.themeKey, theme);
    this.updateToggleButton(theme);
  }

  toggleTheme() {
    const currentTheme = this.htmlElement.getAttribute('data-theme');
    const newTheme = currentTheme === 'light' ? 'dark' : 'light';
    this.setTheme(newTheme);
  }

  updateToggleButton(theme) {
    if (!this.themeToggle) return;
    
    const sunIcon = this.themeToggle.querySelector('.sun-icon');
    const moonIcon = this.themeToggle.querySelector('.moon-icon');
    
    if (theme === 'light') {
      sunIcon.style.display = 'block';
      moonIcon.style.display = 'none';
      this.themeToggle.setAttribute('aria-label', 'Switch to dark mode');
    } else {
      sunIcon.style.display = 'none';
      moonIcon.style.display = 'block';
      this.themeToggle.setAttribute('aria-label', 'Switch to light mode');
    }
  }

  watchSystemTheme() {
    // Check if user prefers dark mode
    const prefersDark = window.matchMedia('(prefers-color-scheme: dark)');
    
    // Only apply system theme if user hasn't made a choice
    if (!localStorage.getItem(this.themeKey)) {
      const systemTheme = prefersDark.matches ? 'dark' : 'light';
      this.setTheme(systemTheme);
    }

    // Listen for system theme changes
    prefersDark.addEventListener('change', (e) => {
      if (!localStorage.getItem(this.themeKey)) {
        const newTheme = e.matches ? 'dark' : 'light';
        this.setTheme(newTheme);
      }
    });
  }

  // Public method to get current theme
  getCurrentTheme() {
    return this.htmlElement.getAttribute('data-theme');
  }

  // Public method to check if dark mode is active
  isDarkMode() {
    return this.getCurrentTheme() === 'dark';
  }
}

// Initialize theme manager when DOM is loaded
document.addEventListener('DOMContentLoaded', () => {
  window.themeManager = new ThemeManager();
  
  // Add smooth transitions after page load
  setTimeout(() => {
    document.body.style.transition = 'background-color 0.3s ease, color 0.3s ease';
  }, 100);
});

// Export for module usage if needed
if (typeof module !== 'undefined' && module.exports) {
  module.exports = ThemeManager;
}
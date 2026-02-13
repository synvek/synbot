// Navigation and UI enhancements
class NavigationManager {
  constructor() {
    this.sidebar = document.querySelector('.sidebar-nav');
    this.currentPage = window.location.pathname;
    this.init();
  }

  init() {
    this.highlightCurrentPage();
    this.makeSidebarCollapsible();
    this.addCopyButtons();
    this.addTableOfContents();
    this.setupSmoothScrolling();
    this.setupSearch();
  }

  highlightCurrentPage() {
    // Highlight current page in sidebar
    const currentLink = document.querySelector(`.sidebar-nav a[href="${this.currentPage}"]`);
    if (currentLink) {
      currentLink.classList.add('active');
      
      // Expand parent section if collapsed
      const parentSection = currentLink.closest('.sidebar-section');
      if (parentSection) {
        parentSection.classList.add('expanded');
      }
    }
  }

  makeSidebarCollapsible() {
    const sectionHeaders = document.querySelectorAll('.sidebar-section h4');
    
    sectionHeaders.forEach(header => {
      // Add click handler to collapse/expand
      header.style.cursor = 'pointer';
      header.addEventListener('click', () => {
        const section = header.parentElement;
        section.classList.toggle('collapsed');
        
        // Toggle icon
        const icon = header.querySelector('.collapse-icon') || this.createCollapseIcon();
        if (!header.querySelector('.collapse-icon')) {
          header.appendChild(icon);
        }
        icon.textContent = section.classList.contains('collapsed') ? '▶' : '▼';
      });
      
      // Add collapse icon
      const icon = this.createCollapseIcon();
      header.appendChild(icon);
    });
  }

  createCollapseIcon() {
    const icon = document.createElement('span');
    icon.className = 'collapse-icon';
    icon.style.marginLeft = '0.5rem';
    icon.style.fontSize = '0.8em';
    icon.textContent = '▼';
    return icon;
  }

  addCopyButtons() {
    // Add copy buttons to code blocks
    const codeBlocks = document.querySelectorAll('pre code');
    
    codeBlocks.forEach(block => {
      const pre = block.parentElement;
      if (pre.querySelector('.copy-button')) return;
      
      const button = document.createElement('button');
      button.className = 'copy-button';
      button.innerHTML = '<i class="far fa-copy"></i>';
      button.setAttribute('aria-label', 'Copy code');
      button.setAttribute('title', 'Copy to clipboard');
      
      button.addEventListener('click', async () => {
        const text = block.textContent;
        try {
          await navigator.clipboard.writeText(text);
          button.innerHTML = '<i class="fas fa-check"></i>';
          button.classList.add('copied');
          
          setTimeout(() => {
            button.innerHTML = '<i class="far fa-copy"></i>';
            button.classList.remove('copied');
          }, 2000);
        } catch (err) {
          console.error('Failed to copy:', err);
          button.innerHTML = '<i class="fas fa-times"></i>';
          button.classList.add('error');
          
          setTimeout(() => {
            button.innerHTML = '<i class="far fa-copy"></i>';
            button.classList.remove('error');
          }, 2000);
        }
      });
      
      pre.style.position = 'relative';
      pre.appendChild(button);
    });
  }

  addTableOfContents() {
    const content = document.querySelector('.content');
    if (!content) return;
    
    const headings = content.querySelectorAll('h2, h3');
    if (headings.length < 3) return; // Only add TOC if there are enough headings
    
    const toc = document.createElement('div');
    toc.className = 'table-of-contents';
    toc.innerHTML = '<h3>Table of Contents</h3><ul></ul>';
    
    const tocList = toc.querySelector('ul');
    let currentH2 = null;
    
    headings.forEach(heading => {
      // Create ID if not exists
      if (!heading.id) {
        heading.id = heading.textContent
          .toLowerCase()
          .replace(/[^\w\s-]/g, '')
          .replace(/\s+/g, '-');
      }
      
      const listItem = document.createElement('li');
      const link = document.createElement('a');
      link.href = `#${heading.id}`;
      link.textContent = heading.textContent;
      
      if (heading.tagName === 'H2') {
        listItem.className = 'toc-h2';
        currentH2 = listItem;
        tocList.appendChild(listItem);
      } else if (heading.tagName === 'H3' && currentH2) {
        listItem.className = 'toc-h3';
        let sublist = currentH2.querySelector('ul');
        if (!sublist) {
          sublist = document.createElement('ul');
          currentH2.appendChild(sublist);
        }
        sublist.appendChild(listItem);
      }
      
      listItem.appendChild(link);
    });
    
    // Insert TOC after first paragraph or at beginning
    const firstParagraph = content.querySelector('p');
    if (firstParagraph) {
      firstParagraph.parentNode.insertBefore(toc, firstParagraph.nextSibling);
    } else {
      content.insertBefore(toc, content.firstChild);
    }
    
    // Add styles for TOC
    this.addTocStyles();
  }

  addTocStyles() {
    const style = document.createElement('style');
    style.textContent = `
      .table-of-contents {
        background-color: var(--bg-secondary);
        border: 1px solid var(--border-color);
        border-radius: 8px;
        padding: 1.5rem;
        margin: 2rem 0;
      }
      
      .table-of-contents h3 {
        margin-top: 0;
        color: var(--text-primary);
        border-bottom: 1px solid var(--border-color);
        padding-bottom: 0.5rem;
      }
      
      .table-of-contents ul {
        list-style: none;
        padding: 0;
        margin: 0;
      }
      
      .table-of-contents li {
        margin: 0.5rem 0;
      }
      
      .table-of-contents a {
        color: var(--text-secondary);
        text-decoration: none;
        transition: color 0.2s ease;
        display: block;
        padding: 0.25rem 0;
      }
      
      .table-of-contents a:hover {
        color: var(--accent-color);
      }
      
      .toc-h2 {
        font-weight: 600;
      }
      
      .toc-h3 {
        padding-left: 1rem;
        font-size: 0.9em;
      }
      
      .copy-button {
        position: absolute;
        top: 0.5rem;
        right: 0.5rem;
        background-color: var(--bg-tertiary);
        border: 1px solid var(--border-color);
        border-radius: 4px;
        padding: 0.25rem 0.5rem;
        cursor: pointer;
        color: var(--text-secondary);
        transition: all 0.2s ease;
        opacity: 0;
      }
      
      pre:hover .copy-button {
        opacity: 1;
      }
      
      .copy-button:hover {
        background-color: var(--bg-primary);
        color: var(--text-primary);
      }
      
      .copy-button.copied {
        color: var(--success-color);
      }
      
      .copy-button.error {
        color: var(--error-color);
      }
      
      .sidebar-section.collapsed ul {
        display: none;
      }
      
      .sidebar-section.collapsed .collapse-icon {
        transform: rotate(-90deg);
      }
    `;
    document.head.appendChild(style);
  }

  setupSmoothScrolling() {
    // Smooth scroll for anchor links
    document.querySelectorAll('a[href^="#"]').forEach(anchor => {
      anchor.addEventListener('click', function (e) {
        const href = this.getAttribute('href');
        if (href === '#') return;
        
        const target = document.querySelector(href);
        if (target) {
          e.preventDefault();
          target.scrollIntoView({
            behavior: 'smooth',
            block: 'start'
          });
          
          // Update URL without scrolling
          history.pushState(null, null, href);
        }
      });
    });
  }

  setupSearch() {
    // Simple client-side search
    const searchInput = document.createElement('input');
    searchInput.type = 'search';
    searchInput.placeholder = 'Search documentation...';
    searchInput.className = 'search-input';
    
    const searchContainer = document.createElement('div');
    searchContainer.className = 'search-container';
    searchContainer.appendChild(searchInput);
    
    // Insert search in sidebar
    const sidebarContent = document.querySelector('.sidebar-content');
    if (sidebarContent) {
      sidebarContent.insertBefore(searchContainer, sidebarContent.firstChild);
      
      searchInput.addEventListener('input', (e) => {
        this.performSearch(e.target.value);
      });
      
      // Add search styles
      this.addSearchStyles();
    }
  }

  addSearchStyles() {
    const style = document.createElement('style');
    style.textContent = `
      .search-container {
        margin-bottom: 1.5rem;
      }
      
      .search-input {
        width: 100%;
        padding: 0.75rem;
        border: 1px solid var(--border-color);
        border-radius: 6px;
        background-color: var(--bg-primary);
        color: var(--text-primary);
        font-size: 0.9rem;
        transition: border-color 0.2s ease;
      }
      
      .search-input:focus {
        outline: none;
        border-color: var(--accent-color);
        box-shadow: 0 0 0 3px rgba(3, 102, 214, 0.1);
      }
      
      .search-input::placeholder {
        color: var(--text-tertiary);
      }
      
      .search-highlight {
        background-color: rgba(255, 193, 7, 0.3);
        padding: 0.1em 0.2em;
        border-radius: 2px;
      }
    `;
    document.head.appendChild(style);
  }

  performSearch(query) {
    if (!query.trim()) {
      // Clear highlights
      document.querySelectorAll('.search-highlight').forEach(el => {
        const parent = el.parentNode;
        parent.replaceChild(document.createTextNode(el.textContent), el);
        parent.normalize();
      });
      return;
    }
    
    const content = document.querySelector('.content');
    if (!content) return;
    
    // Clear previous highlights
    document.querySelectorAll('.search-highlight').forEach(el => {
      const parent = el.parentNode;
      parent.replaceChild(document.createTextNode(el.textContent), el);
      parent.normalize();
    });
    
    // Search in text nodes
    const walker = document.createTreeWalker(
      content,
      NodeFilter.SHOW_TEXT,
      null,
      false
    );
    
    const regex = new RegExp(`(${this.escapeRegex(query)})`, 'gi');
    const nodes = [];
    let node;
    
    while (node = walker.nextNode()) {
      if (node.textContent.match(regex)) {
        nodes.push(node);
      }
    }
    
    // Highlight matches
    nodes.forEach(textNode => {
      const fragment = document.createDocumentFragment();
      const parts = textNode.textContent.split(regex);
      
      parts.forEach(part => {
        if (part.match(regex)) {
          const span = document.createElement('span');
          span.className = 'search-highlight';
          span.textContent = part;
          fragment.appendChild(span);
        } else if (part) {
          fragment.appendChild(document.createTextNode(part));
        }
      });
      
      textNode.parentNode.replaceChild(fragment, textNode);
    });
  }

  escapeRegex(string) {
    return string.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  }
}

// Initialize navigation manager when DOM is loaded
document.addEventListener('DOMContentLoaded', () => {
  window.navigationManager = new NavigationManager();
  
  // Add loading animation
  document.body.classList.add('loaded');
});

// Export for module usage if needed
if (typeof module !== 'undefined' && module.exports) {
  module.exports = NavigationManager;
}
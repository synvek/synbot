import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import Markdown from './Markdown';

describe('Markdown', () => {
  it('renders headings, bold text and inline code', () => {
    render(<Markdown content={'## 标题\n\n这是 **加粗** 与 `code`。'} />);
    expect(screen.getByRole('heading', { name: '标题' })).toBeTruthy();
    expect(screen.getByText('加粗').tagName).toBe('STRONG');
    expect(screen.getByText('code').tagName).toBe('CODE');
  });

  it('renders fenced code blocks with a language label and copy action', () => {
    render(<Markdown content={'```rust\nfn main() {}\n```'} />);
    expect(screen.getByText('rust')).toBeTruthy();
    expect(screen.getByLabelText('Copy code')).toBeTruthy();
    expect(screen.getByText(/fn/)).toBeTruthy();
  });

  it('renders unordered lists as list items', () => {
    render(<Markdown content={'- 第一项\n- 第二项'} />);
    expect(screen.getAllByRole('listitem')).toHaveLength(2);
  });

  it('renders tables from pipe syntax', () => {
    render(<Markdown content={'| a | b |\n| --- | --- |\n| 1 | 2 |'} />);
    expect(screen.getByRole('table')).toBeTruthy();
    expect(screen.getByText('1')).toBeTruthy();
  });

  it('only links http(s) hrefs and keeps raw html as text', () => {
    render(
      <Markdown content={'[ok](https://example.com) [bad](javascript:alert(1)) <img src=x>'} />
    );
    const link = screen.getByRole('link', { name: 'ok' });
    expect(link.getAttribute('href')).toBe('https://example.com');
    // `javascript:` links degrade to text instead of anchor tags.
    expect(screen.queryByRole('link', { name: 'bad' })).toBeNull();
    expect(screen.getByText(/<img src=x>/)).toBeTruthy();
    expect(document.querySelector('img')).toBeNull();
  });
});

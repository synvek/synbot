import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import ApprovalRequest from './ApprovalRequest';
import { I18nProvider } from '../i18n/I18nContext';
import type { ApprovalRequest as ApprovalRequestType } from '../types/websocket';

// Mock approval request data
const mockRequest: ApprovalRequestType = {
  id: 'test-request-123',
  command: 'rm -rf /tmp/test',
  working_dir: '/home/user/project',
  context: 'User requested to clean temporary files',
  timestamp: '2024-01-15T10:30:00Z',
  timeout_secs: 300,
};

// Wrapper component with I18n provider
const renderWithI18n = (component: React.ReactElement) => {
  return render(
    <I18nProvider>
      {component}
    </I18nProvider>
  );
};

describe('ApprovalRequest Component', () => {
  it('renders approval request information correctly', () => {
    const onApprove = vi.fn();
    const onReject = vi.fn();

    renderWithI18n(
      <ApprovalRequest
        request={mockRequest}
        onApprove={onApprove}
        onReject={onReject}
      />
    );

    // Check if command is displayed
    expect(screen.getByText(mockRequest.command)).toBeInTheDocument();
    
    // Check if working directory is displayed
    expect(screen.getByText(mockRequest.working_dir)).toBeInTheDocument();
    
    // Check if context is displayed
    expect(screen.getByText(mockRequest.context)).toBeInTheDocument();
  });

  it('calls onApprove when approve button is clicked', () => {
    const onApprove = vi.fn();
    const onReject = vi.fn();

    renderWithI18n(
      <ApprovalRequest
        request={mockRequest}
        onApprove={onApprove}
        onReject={onReject}
      />
    );

    const approveButton = screen.getByRole('button', { name: /approve|同意/i });
    fireEvent.click(approveButton);

    expect(onApprove).toHaveBeenCalledWith(mockRequest.id);
    expect(onApprove).toHaveBeenCalledTimes(1);
  });

  it('calls onReject when reject button is clicked', () => {
    const onApprove = vi.fn();
    const onReject = vi.fn();

    renderWithI18n(
      <ApprovalRequest
        request={mockRequest}
        onApprove={onApprove}
        onReject={onReject}
      />
    );

    const rejectButton = screen.getByRole('button', { name: /reject|拒绝/i });
    fireEvent.click(rejectButton);

    expect(onReject).toHaveBeenCalledWith(mockRequest.id);
    expect(onReject).toHaveBeenCalledTimes(1);
  });

  it('hides buttons and shows result when approval is processed', () => {
    const onApprove = vi.fn();
    const onReject = vi.fn();

    renderWithI18n(
      <ApprovalRequest
        request={mockRequest}
        onApprove={onApprove}
        onReject={onReject}
        result={{ approved: true, message: 'Command approved' }}
      />
    );

    // Buttons should not be present when result is shown
    expect(screen.queryByRole('button', { name: /approve|同意/i })).not.toBeInTheDocument();
    expect(screen.queryByRole('button', { name: /reject|拒绝/i })).not.toBeInTheDocument();
  });

  it('displays approval result feedback when provided', () => {
    const onApprove = vi.fn();
    const onReject = vi.fn();

    const { rerender } = renderWithI18n(
      <ApprovalRequest
        request={mockRequest}
        onApprove={onApprove}
        onReject={onReject}
        result={{ approved: true, message: 'Command approved successfully' }}
      />
    );

    expect(screen.getByText('Command approved successfully')).toBeInTheDocument();

    // Test rejection result
    rerender(
      <I18nProvider>
        <ApprovalRequest
          request={mockRequest}
          onApprove={onApprove}
          onReject={onReject}
          result={{ approved: false, message: 'Command rejected by user' }}
        />
      </I18nProvider>
    );

    expect(screen.getByText('Command rejected by user')).toBeInTheDocument();
  });

  it('prevents multiple clicks on approve button', () => {
    const onApprove = vi.fn();
    const onReject = vi.fn();

    renderWithI18n(
      <ApprovalRequest
        request={mockRequest}
        onApprove={onApprove}
        onReject={onReject}
      />
    );

    const approveButton = screen.getByRole('button', { name: /approve|同意/i });
    
    // Click multiple times rapidly
    fireEvent.click(approveButton);
    fireEvent.click(approveButton);
    fireEvent.click(approveButton);

    // Should only be called once due to disabled state after first click
    expect(onApprove).toHaveBeenCalledTimes(1);
  });
});

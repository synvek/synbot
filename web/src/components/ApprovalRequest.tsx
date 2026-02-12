import { useState } from 'react';
import { ApprovalRequest as ApprovalRequestType } from '../types/websocket';
import { useI18n } from '../i18n/I18nContext';

interface ApprovalRequestProps {
  request: ApprovalRequestType;
  onApprove: (requestId: string) => void;
  onReject: (requestId: string) => void;
  result?: {
    approved: boolean;
    message: string;
  };
}

export default function ApprovalRequest({
  request,
  onApprove,
  onReject,
  result,
}: ApprovalRequestProps) {
  const [responded, setResponded] = useState(!!result);
  const { t } = useI18n();

  const handleApprove = () => {
    onApprove(request.id);
    setResponded(true);
  };

  const handleReject = () => {
    onReject(request.id);
    setResponded(true);
  };

  const formatTimestamp = (timestamp: string) => {
    return new Date(timestamp).toLocaleString();
  };

  return (
    <div className="bg-surface border-2 border-warning rounded-lg shadow-lg p-4 max-w-2xl">
      {/* Header */}
      <div className="flex items-center gap-2 mb-4 pb-3 border-b border-border">
        <svg
          className="w-6 h-6 text-warning"
          fill="none"
          stroke="currentColor"
          viewBox="0 0 24 24"
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            strokeWidth={2}
            d="M12 15v2m-6 4h12a2 2 0 002-2v-6a2 2 0 00-2-2H6a2 2 0 00-2 2v6a2 2 0 002 2zm10-10V7a4 4 0 00-8 0v4h8z"
          />
        </svg>
        <h3 className="text-lg font-semibold text-text">
          {t('approval.title', '命令执行审批请求')}
        </h3>
      </div>

      {/* Request Details */}
      <div className="space-y-3 mb-4">
        <div>
          <label className="text-sm font-medium text-text-secondary block mb-1">
            {t('approval.command', '命令')}:
          </label>
          <code className="block bg-background border border-border rounded px-3 py-2 text-sm text-text font-mono break-all">
            {request.command}
          </code>
        </div>

        <div>
          <label className="text-sm font-medium text-text-secondary block mb-1">
            {t('approval.workingDir', '工作目录')}:
          </label>
          <code className="block bg-background border border-border rounded px-3 py-2 text-sm text-text font-mono">
            {request.working_dir}
          </code>
        </div>

        <div>
          <label className="text-sm font-medium text-text-secondary block mb-1">
            {t('approval.context', '上下文')}:
          </label>
          <div className="bg-background border border-border rounded px-3 py-2 text-sm text-text whitespace-pre-wrap">
            {request.context}
          </div>
        </div>

        <div className="flex items-center justify-between text-sm text-text-secondary">
          <span>
            {t('approval.requestTime', '请求时间')}: {formatTimestamp(request.timestamp)}
          </span>
          <span>
            {t('approval.timeout', '超时')}: {request.timeout_secs}s
          </span>
        </div>
      </div>

      {/* Action Buttons or Result */}
      {!responded && !result ? (
        <div className="flex gap-3">
          <button
            onClick={handleApprove}
            className="flex-1 px-4 py-2 bg-success text-white rounded-lg hover:bg-success/90 transition-colors font-medium flex items-center justify-center gap-2"
          >
            <svg
              className="w-5 h-5"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M5 13l4 4L19 7"
              />
            </svg>
            {t('approval.approve', '同意')}
          </button>
          <button
            onClick={handleReject}
            className="flex-1 px-4 py-2 bg-error text-white rounded-lg hover:bg-error/90 transition-colors font-medium flex items-center justify-center gap-2"
          >
            <svg
              className="w-5 h-5"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M6 18L18 6M6 6l12 12"
              />
            </svg>
            {t('approval.reject', '不同意')}
          </button>
        </div>
      ) : (
        <div
          className={`px-4 py-3 rounded-lg flex items-center gap-2 ${
            result?.approved
              ? 'bg-success/10 border border-success/30 text-success'
              : 'bg-error/10 border border-error/30 text-error'
          }`}
        >
          {result?.approved ? (
            <svg
              className="w-5 h-5 flex-shrink-0"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z"
              />
            </svg>
          ) : (
            <svg
              className="w-5 h-5 flex-shrink-0"
              fill="none"
              stroke="currentColor"
              viewBox="0 0 24 24"
            >
              <path
                strokeLinecap="round"
                strokeLinejoin="round"
                strokeWidth={2}
                d="M10 14l2-2m0 0l2-2m-2 2l-2-2m2 2l2 2m7-2a9 9 0 11-18 0 9 9 0 0118 0z"
              />
            </svg>
          )}
          <span className="font-medium">
            {result?.message ||
              (result?.approved
                ? t('approval.approved', '已批准')
                : t('approval.rejected', '已拒绝'))}
          </span>
        </div>
      )}
    </div>
  );
}

import { useState, FormEvent } from 'react';
import { Navigate, useNavigate } from 'react-router-dom';
import { apiClient } from '../api/client';
import { useI18n } from '../i18n/I18nContext';
import { Button, Field, Input } from '../ui';

export default function Login() {
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(false);
  const navigate = useNavigate();
  const { t } = useI18n();

  // The desktop shell bootstraps a per-launch token over IPC; there is no password to ask for.
  if (apiClient.isDesktop()) {
    return <Navigate to="/" replace />;
  }

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault();
    setError('');
    setLoading(true);

    try {
      // Set auth credentials
      apiClient.setAuth(username, password);

      // Verify credentials by making a test API call
      await apiClient.getStatus();

      // If successful, navigate to home
      navigate('/');
    } catch {
      // Clear invalid credentials
      apiClient.clearAuth();
      setError(t('login.invalidCredentials'));
    } finally {
      setLoading(false);
    }
  };

  return (
    <main className="flex min-h-screen items-center justify-center bg-background px-6 text-ink">
      <div className="w-full max-w-[352px]">
        <div className="mb-8 flex items-center gap-2">
          <span className="flex h-7 w-7 items-center justify-center rounded-control bg-ink text-small font-semibold text-background">
            S
          </span>
          <span className="text-section font-semibold">Synbot</span>
        </div>

        <h1 className="text-display font-semibold">{t('login.title', 'Sign in')}</h1>
        <p className="mt-1 text-small text-ink-2">
          {t('login.subtitle', 'Sign in to manage this daemon.')}
        </p>

        <form onSubmit={handleSubmit} className="mt-6 space-y-4">
          <Field label={t('common.username', 'Username')} htmlFor="username" required>
            <Input
              id="username"
              type="text"
              autoComplete="username"
              autoFocus
              value={username}
              onChange={(e) => setUsername(e.target.value)}
              required
              disabled={loading}
            />
          </Field>

          <Field label={t('common.password', 'Password')} htmlFor="password" required>
            <Input
              id="password"
              type="password"
              autoComplete="current-password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              required
              disabled={loading}
            />
          </Field>

          {error && (
            <p role="alert" className="rounded-control border px-2.5 py-2 text-small tone-danger">
              {error}
            </p>
          )}

          <Button type="submit" variant="primary" className="w-full" loading={loading}>
            {loading ? t('login.loggingIn') : t('common.login')}
          </Button>
        </form>
      </div>
    </main>
  );
}

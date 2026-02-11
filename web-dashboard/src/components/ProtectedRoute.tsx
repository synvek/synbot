import { Navigate } from 'react-router-dom';
import { apiClient } from '../api/client';

interface ProtectedRouteProps {
  children: React.ReactNode;
}

export default function ProtectedRoute({ children }: ProtectedRouteProps) {
  // Try to load auth from session storage
  if (!apiClient.isAuthenticated()) {
    apiClient.loadAuth();
  }

  // If still not authenticated, redirect to login
  if (!apiClient.isAuthenticated()) {
    return <Navigate to="/login" replace />;
  }

  return <>{children}</>;
}

/**
 * LoginScreen - Medplum Authentication UI
 *
 * Displays a login button when user is not authenticated.
 * Shows loading state during OAuth flow.
 */

import { useAuth } from './AuthProvider';

interface LoginScreenProps {
  onSkip?: () => void;
}

export function LoginScreen({ onSkip }: LoginScreenProps) {
  const { isLoading, error, login } = useAuth();

  return (
    <div className="login-screen">
      <div className="login-header">
        <h2>Welcome to Scribe</h2>
        <p className="login-subtitle">Sign in to sync encounters with your EMR</p>
      </div>

      <div className="login-content">
        <button
          className="login-button"
          onClick={login}
          disabled={isLoading}
        >
          {isLoading ? (
            <>
              <span className="spinner" />
              Connecting...
            </>
          ) : (
            'Sign in with Medplum'
          )}
        </button>

        {error && (
          <div className="login-error">
            {error}
          </div>
        )}

        {onSkip && (
          <button
            className="skip-login-button"
            onClick={onSkip}
            disabled={isLoading}
          >
            Continue without signing in
          </button>
        )}
      </div>

      <div className="login-footer">
        <p className="login-hint">
          You can also configure Medplum settings in the settings drawer
        </p>
      </div>
    </div>
  );
}

export default LoginScreen;

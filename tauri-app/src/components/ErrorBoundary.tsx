import { Component, ErrorInfo, ReactNode } from 'react';

interface Props {
  children: ReactNode;
  fallback?: ReactNode;
  onError?: (error: Error, errorInfo: ErrorInfo) => void;
}

interface State {
  hasError: boolean;
  error: Error | null;
}

/**
 * Error Boundary component that catches JavaScript errors anywhere in the
 * child component tree and displays a fallback UI instead of crashing.
 *
 * Usage:
 * ```tsx
 * <ErrorBoundary>
 *   <MyComponent />
 * </ErrorBoundary>
 * ```
 *
 * With custom fallback:
 * ```tsx
 * <ErrorBoundary fallback={<div>Something went wrong</div>}>
 *   <MyComponent />
 * </ErrorBoundary>
 * ```
 */
export class ErrorBoundary extends Component<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = { hasError: false, error: null };
  }

  static getDerivedStateFromError(error: Error): State {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: ErrorInfo): void {
    console.error('ErrorBoundary caught an error:', error, errorInfo);
    this.props.onError?.(error, errorInfo);
  }

  handleRetry = (): void => {
    this.setState({ hasError: false, error: null });
  };

  render(): ReactNode {
    if (this.state.hasError) {
      if (this.props.fallback) {
        return this.props.fallback;
      }

      return (
        <div className="error-boundary" role="alert">
          <div className="error-boundary-content">
            <h3>Something went wrong</h3>
            <p className="error-message">
              {this.state.error?.message || 'An unexpected error occurred'}
            </p>
            <button
              className="error-retry-button"
              onClick={this.handleRetry}
            >
              Try Again
            </button>
            <details className="error-details">
              <summary>Technical Details</summary>
              <pre>{this.state.error?.stack}</pre>
            </details>
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}

/**
 * Default error fallback component for use with ErrorBoundary
 */
export function ErrorFallback({
  error,
  onRetry,
}: {
  error?: Error;
  onRetry?: () => void;
}): JSX.Element {
  return (
    <div className="error-fallback" role="alert">
      <div className="error-icon" aria-hidden="true">
        ⚠️
      </div>
      <h2>Oops! Something went wrong</h2>
      <p className="error-details">
        {error?.message || 'An unexpected error occurred in the application.'}
      </p>
      {onRetry && (
        <button onClick={onRetry} className="btn btn-primary">
          Retry
        </button>
      )}
      <p className="error-help">
        If this problem persists, please restart the application.
      </p>
    </div>
  );
}

export default ErrorBoundary;

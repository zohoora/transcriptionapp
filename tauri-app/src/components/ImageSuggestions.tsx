import { memo, useState, useCallback, useEffect, useRef } from 'react';
import type { MiisSuggestion } from '../hooks/useMiisImages';
import type { AiImage } from '../hooks/useAiImages';

interface ImageSuggestionsProps {
  suggestions: MiisSuggestion[];
  isLoading: boolean;
  error: string | null;
  getImageUrl: (path: string) => string;
  onImpression: (imageId: number) => void;
  onClickImage: (imageId: number) => void;
  onDismiss: (imageId: number) => void;
  // AI-generated image props
  aiImages?: AiImage[];
  aiLoading?: boolean;
  aiError?: string | null;
  onAiDismiss?: (index: number) => void;
  imageSource?: 'miis' | 'ai' | 'off';
}

/**
 * Displays medical illustration suggestions from MIIS during recording.
 * Shows thumbnails in a horizontal strip with click-to-expand functionality.
 */
export const ImageSuggestions = memo(function ImageSuggestions({
  suggestions,
  isLoading,
  error,
  getImageUrl,
  onImpression,
  onClickImage,
  onDismiss,
  aiImages,
  aiLoading,
  aiError,
  onAiDismiss,
  imageSource = 'miis',
}: ImageSuggestionsProps) {
  const [expandedImage, setExpandedImage] = useState<MiisSuggestion | null>(null);
  const [expandedAiImage, setExpandedAiImage] = useState<AiImage | null>(null);
  const [dismissedIds, setDismissedIds] = useState<Set<number>>(new Set());
  const impressionTracked = useRef<Set<number>>(new Set());

  // Filter out dismissed images
  const visibleSuggestions = suggestions.filter(s => !dismissedIds.has(s.image_id));

  // Track impressions when images become visible
  useEffect(() => {
    visibleSuggestions.forEach(suggestion => {
      if (!impressionTracked.current.has(suggestion.image_id)) {
        impressionTracked.current.add(suggestion.image_id);
        onImpression(suggestion.image_id);
      }
    });
  }, [visibleSuggestions, onImpression]);

  const handleImageClick = useCallback((suggestion: MiisSuggestion) => {
    onClickImage(suggestion.image_id);
    setExpandedImage(suggestion);
  }, [onClickImage]);

  const handleDismiss = useCallback((e: React.MouseEvent, imageId: number) => {
    e.stopPropagation();
    onDismiss(imageId);
    setDismissedIds(prev => new Set([...prev, imageId]));
  }, [onDismiss]);

  const handleCloseExpanded = useCallback(() => {
    setExpandedImage(null);
    setExpandedAiImage(null);
  }, []);

  // AI image rendering path
  if (imageSource === 'ai') {
    const activeLoading = aiLoading ?? false;
    const activeError = aiError ?? null;
    const activeImages = aiImages ?? [];

    if (!activeLoading && activeImages.length === 0 && !activeError) {
      return null;
    }

    return (
      <>
        <div style={{
          padding: '8px 12px',
          borderTop: '1px solid var(--border-color, #e0e0e0)',
          backgroundColor: 'var(--bg-secondary, #f5f5f5)',
        }}>
          <div style={{
            display: 'flex',
            alignItems: 'center',
            gap: '8px',
            marginBottom: '6px',
          }}>
            <span style={{
              fontSize: '11px',
              fontWeight: 500,
              color: 'var(--text-secondary, #666)',
              textTransform: 'uppercase',
              letterSpacing: '0.5px',
            }}>
              AI Illustrations
            </span>
            {activeLoading && (
              <span style={{
                fontSize: '10px',
                color: 'var(--text-tertiary, #999)',
              }}>
                Generating...
              </span>
            )}
          </div>

          {activeError && (
            <div style={{
              fontSize: '11px',
              color: 'var(--error-color, #d32f2f)',
              padding: '4px 0',
            }}>
              {activeError}
            </div>
          )}

          <div style={{
            display: 'flex',
            gap: '8px',
            overflowX: 'auto',
            paddingBottom: '4px',
          }}>
            {activeImages.map((img, index) => (
              <div
                key={img.timestamp}
                onClick={() => setExpandedAiImage(img)}
                style={{
                  position: 'relative',
                  flexShrink: 0,
                  width: '80px',
                  height: '80px',
                  borderRadius: '6px',
                  overflow: 'hidden',
                  cursor: 'pointer',
                  border: '1px solid var(--border-color, #ddd)',
                  backgroundColor: '#fff',
                  transition: 'transform 0.15s, box-shadow 0.15s',
                }}
                onMouseEnter={e => {
                  e.currentTarget.style.transform = 'scale(1.05)';
                  e.currentTarget.style.boxShadow = '0 2px 8px rgba(0,0,0,0.15)';
                }}
                onMouseLeave={e => {
                  e.currentTarget.style.transform = 'scale(1)';
                  e.currentTarget.style.boxShadow = 'none';
                }}
              >
                <img
                  src={`data:image/png;base64,${img.base64}`}
                  alt="AI-generated medical illustration"
                  style={{
                    width: '100%',
                    height: '100%',
                    objectFit: 'cover',
                  }}
                />
                <button
                  onClick={e => {
                    e.stopPropagation();
                    onAiDismiss?.(index);
                  }}
                  style={{
                    position: 'absolute',
                    top: '2px',
                    right: '2px',
                    width: '18px',
                    height: '18px',
                    borderRadius: '50%',
                    border: 'none',
                    backgroundColor: 'rgba(0,0,0,0.5)',
                    color: '#fff',
                    fontSize: '12px',
                    lineHeight: '16px',
                    cursor: 'pointer',
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'center',
                    opacity: 0.7,
                    transition: 'opacity 0.15s',
                  }}
                  onMouseEnter={e => { e.currentTarget.style.opacity = '1'; }}
                  onMouseLeave={e => { e.currentTarget.style.opacity = '0.7'; }}
                  title="Dismiss"
                >
                  ×
                </button>
              </div>
            ))}
          </div>
        </div>

        {/* Expanded AI image modal */}
        {expandedAiImage && (
          <div
            onClick={handleCloseExpanded}
            style={{
              position: 'fixed',
              top: 0, left: 0, right: 0, bottom: 0,
              backgroundColor: 'rgba(0,0,0,0.8)',
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              zIndex: 9999,
              padding: '20px',
            }}
          >
            <div
              onClick={e => e.stopPropagation()}
              style={{
                maxWidth: '90vw',
                maxHeight: '90vh',
                backgroundColor: '#fff',
                borderRadius: '8px',
                overflow: 'hidden',
                boxShadow: '0 4px 20px rgba(0,0,0,0.3)',
              }}
            >
              <div style={{
                padding: '12px 16px',
                borderBottom: '1px solid #eee',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'space-between',
              }}>
                <h3 style={{
                  margin: 0,
                  fontSize: '16px',
                  fontWeight: 600,
                  color: '#333',
                }}>
                  AI Medical Illustration
                </h3>
                <button
                  onClick={handleCloseExpanded}
                  style={{
                    width: '32px',
                    height: '32px',
                    borderRadius: '50%',
                    border: 'none',
                    backgroundColor: '#f0f0f0',
                    color: '#666',
                    fontSize: '20px',
                    cursor: 'pointer',
                    display: 'flex',
                    alignItems: 'center',
                    justifyContent: 'center',
                  }}
                >
                  ×
                </button>
              </div>
              <div style={{
                padding: '16px',
                display: 'flex',
                alignItems: 'center',
                justifyContent: 'center',
                backgroundColor: '#f9f9f9',
              }}>
                <img
                  src={`data:image/png;base64,${expandedAiImage.base64}`}
                  alt="AI-generated medical illustration"
                  style={{
                    maxWidth: '100%',
                    maxHeight: 'calc(90vh - 100px)',
                    objectFit: 'contain',
                  }}
                />
              </div>
            </div>
          </div>
        )}
      </>
    );
  }

  // MIIS rendering path (default)
  // Don't render if no suggestions and not loading
  if (!isLoading && visibleSuggestions.length === 0 && !error) {
    return null;
  }

  return (
    <>
      {/* Thumbnail strip */}
      <div style={{
        padding: '8px 12px',
        borderTop: '1px solid var(--border-color, #e0e0e0)',
        backgroundColor: 'var(--bg-secondary, #f5f5f5)',
      }}>
        <div style={{
          display: 'flex',
          alignItems: 'center',
          gap: '8px',
          marginBottom: '6px',
        }}>
          <span style={{
            fontSize: '11px',
            fontWeight: 500,
            color: 'var(--text-secondary, #666)',
            textTransform: 'uppercase',
            letterSpacing: '0.5px',
          }}>
            Related Images
          </span>
          {isLoading && (
            <span style={{
              fontSize: '10px',
              color: 'var(--text-tertiary, #999)',
            }}>
              Loading...
            </span>
          )}
        </div>

        {error && (
          <div style={{
            fontSize: '11px',
            color: 'var(--error-color, #d32f2f)',
            padding: '4px 0',
          }}>
            {error}
          </div>
        )}

        <div style={{
          display: 'flex',
          gap: '8px',
          overflowX: 'auto',
          paddingBottom: '4px',
        }}>
          {visibleSuggestions.map(suggestion => (
            <div
              key={suggestion.image_id}
              onClick={() => handleImageClick(suggestion)}
              style={{
                position: 'relative',
                flexShrink: 0,
                width: '80px',
                height: '80px',
                borderRadius: '6px',
                overflow: 'hidden',
                cursor: 'pointer',
                border: '1px solid var(--border-color, #ddd)',
                backgroundColor: '#fff',
                transition: 'transform 0.15s, box-shadow 0.15s',
              }}
              onMouseEnter={e => {
                e.currentTarget.style.transform = 'scale(1.05)';
                e.currentTarget.style.boxShadow = '0 2px 8px rgba(0,0,0,0.15)';
              }}
              onMouseLeave={e => {
                e.currentTarget.style.transform = 'scale(1)';
                e.currentTarget.style.boxShadow = 'none';
              }}
            >
              <img
                src={getImageUrl(suggestion.thumb_url)}
                alt={suggestion.title || 'Medical illustration'}
                loading="lazy"
                style={{
                  width: '100%',
                  height: '100%',
                  objectFit: 'cover',
                }}
                onError={e => {
                  // Hide broken images
                  (e.target as HTMLImageElement).style.display = 'none';
                }}
              />
              {/* Dismiss button */}
              <button
                onClick={e => handleDismiss(e, suggestion.image_id)}
                style={{
                  position: 'absolute',
                  top: '2px',
                  right: '2px',
                  width: '18px',
                  height: '18px',
                  borderRadius: '50%',
                  border: 'none',
                  backgroundColor: 'rgba(0,0,0,0.5)',
                  color: '#fff',
                  fontSize: '12px',
                  lineHeight: '16px',
                  cursor: 'pointer',
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'center',
                  opacity: 0.7,
                  transition: 'opacity 0.15s',
                }}
                onMouseEnter={e => { e.currentTarget.style.opacity = '1'; }}
                onMouseLeave={e => { e.currentTarget.style.opacity = '0.7'; }}
                title="Dismiss"
              >
                ×
              </button>
            </div>
          ))}
        </div>
      </div>

      {/* Expanded image modal */}
      {expandedImage && (
        <div
          onClick={handleCloseExpanded}
          style={{
            position: 'fixed',
            top: 0,
            left: 0,
            right: 0,
            bottom: 0,
            backgroundColor: 'rgba(0,0,0,0.8)',
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'center',
            zIndex: 9999,
            padding: '20px',
          }}
        >
          <div
            onClick={e => e.stopPropagation()}
            style={{
              maxWidth: '90vw',
              maxHeight: '90vh',
              backgroundColor: '#fff',
              borderRadius: '8px',
              overflow: 'hidden',
              boxShadow: '0 4px 20px rgba(0,0,0,0.3)',
            }}
          >
            <div style={{
              padding: '12px 16px',
              borderBottom: '1px solid #eee',
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'space-between',
            }}>
              <div>
                <h3 style={{
                  margin: 0,
                  fontSize: '16px',
                  fontWeight: 600,
                  color: '#333',
                }}>
                  {expandedImage.title || 'Medical Illustration'}
                </h3>
                {expandedImage.description && (
                  <p style={{
                    margin: '4px 0 0',
                    fontSize: '13px',
                    color: '#666',
                  }}>
                    {expandedImage.description}
                  </p>
                )}
              </div>
              <button
                onClick={handleCloseExpanded}
                style={{
                  width: '32px',
                  height: '32px',
                  borderRadius: '50%',
                  border: 'none',
                  backgroundColor: '#f0f0f0',
                  color: '#666',
                  fontSize: '20px',
                  cursor: 'pointer',
                  display: 'flex',
                  alignItems: 'center',
                  justifyContent: 'center',
                }}
              >
                ×
              </button>
            </div>
            <div style={{
              padding: '16px',
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'center',
              backgroundColor: '#f9f9f9',
            }}>
              <img
                src={getImageUrl(expandedImage.display_url)}
                alt={expandedImage.title || 'Medical illustration'}
                style={{
                  maxWidth: '100%',
                  maxHeight: 'calc(90vh - 100px)',
                  objectFit: 'contain',
                }}
              />
            </div>
          </div>
        </div>
      )}
    </>
  );
});

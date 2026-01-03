import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useSoapNote } from './useSoapNote';
import { invoke } from '@tauri-apps/api/core';

// Type the mock from global setup
const mockInvoke = vi.mocked(invoke);

describe('useSoapNote', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    // Reset mock to default resolved value to prevent state bleeding between tests
    mockInvoke.mockReset();
    mockInvoke.mockResolvedValue(undefined);
  });

  it('initializes with correct default state', () => {
    const { result } = renderHook(() => useSoapNote());

    expect(result.current.isGeneratingSoap).toBe(false);
    expect(result.current.soapError).toBeNull();
    expect(result.current.ollamaStatus).toBeNull();
    expect(result.current.ollamaModels).toEqual([]);
  });

  it('generates SOAP note successfully', async () => {
    const mockSoapNote = {
      subjective: 'Patient reports symptoms',
      objective: 'Vitals normal',
      assessment: 'Common cold',
      plan: 'Rest and fluids',
      generated_at: '2025-01-01T00:00:00Z',
      model_used: 'qwen3:4b',
    };

    mockInvoke.mockResolvedValue(mockSoapNote);

    const { result } = renderHook(() => useSoapNote());

    let soapResult;
    await act(async () => {
      soapResult = await result.current.generateSoapNote('Patient said they feel sick');
    });

    expect(mockInvoke).toHaveBeenCalledWith('generate_soap_note', {
      transcript: 'Patient said they feel sick',
      audioEvents: undefined, // Optional audio events parameter
    });
    expect(soapResult).toEqual(mockSoapNote);
    expect(result.current.soapError).toBeNull();
    expect(result.current.isGeneratingSoap).toBe(false);
  });

  it('passes audio events to generate_soap_note', async () => {
    const mockSoapNote = {
      subjective: 'Patient reports cough',
      objective: 'Observed coughing during visit',
      assessment: 'Respiratory infection',
      plan: 'Cough suppressant',
      generated_at: '2025-01-01T00:00:00Z',
      model_used: 'qwen3:4b',
    };

    const audioEvents = [
      { timestamp_ms: 30000, duration_ms: 500, confidence: 2.0, label: 'Cough' },
      { timestamp_ms: 45000, duration_ms: 300, confidence: 1.8, label: 'Throat clearing' },
    ];

    mockInvoke.mockResolvedValue(mockSoapNote);

    const { result } = renderHook(() => useSoapNote());

    let soapResult;
    await act(async () => {
      soapResult = await result.current.generateSoapNote('Patient has a cough', audioEvents);
    });

    expect(mockInvoke).toHaveBeenCalledWith('generate_soap_note', {
      transcript: 'Patient has a cough',
      audioEvents: audioEvents,
    });
    expect(soapResult).toEqual(mockSoapNote);
  });

  it('returns null for empty transcript', async () => {
    const { result } = renderHook(() => useSoapNote());

    let soapResult;
    await act(async () => {
      soapResult = await result.current.generateSoapNote('');
    });

    expect(soapResult).toBeNull();
    expect(mockInvoke).not.toHaveBeenCalled();
  });

  it('returns null for whitespace-only transcript', async () => {
    const { result } = renderHook(() => useSoapNote());

    let soapResult;
    await act(async () => {
      soapResult = await result.current.generateSoapNote('   \n\t  ');
    });

    expect(soapResult).toBeNull();
    expect(mockInvoke).not.toHaveBeenCalled();
  });

  it('handles generation error gracefully', async () => {
    mockInvoke.mockRejectedValue(new Error('Ollama not available'));

    const { result } = renderHook(() => useSoapNote());

    let soapResult;
    await act(async () => {
      soapResult = await result.current.generateSoapNote('Test transcript');
    });

    expect(soapResult).toBeNull();
    expect(result.current.soapError).toBe('Ollama not available');
    expect(result.current.isGeneratingSoap).toBe(false);
  });

  it('sets isGeneratingSoap during generation', async () => {
    let resolveGeneration: (value: unknown) => void;
    const generationPromise = new Promise((resolve) => {
      resolveGeneration = resolve;
    });

    mockInvoke.mockReturnValue(generationPromise);

    const { result } = renderHook(() => useSoapNote());

    // Start generation - note: we trigger the call inside act but don't await the result yet
    let generatePromise: Promise<unknown>;
    act(() => {
      // Don't await here - just trigger the async operation
      generatePromise = result.current.generateSoapNote('Test');
    });

    // Should be generating (state is set synchronously before the await)
    expect(result.current.isGeneratingSoap).toBe(true);

    // Now complete the async operation and wait for it
    await act(async () => {
      resolveGeneration!({
        subjective: 'test',
        objective: 'test',
        assessment: 'test',
        plan: 'test',
        generated_at: '2025-01-01T00:00:00Z',
        model_used: 'test',
      });
      await generatePromise;
    });

    expect(result.current.isGeneratingSoap).toBe(false);
  });

  it('clears previous error on new generation', async () => {
    mockInvoke
      .mockRejectedValueOnce(new Error('First error'))
      .mockResolvedValueOnce({
        subjective: 'test',
        objective: 'test',
        assessment: 'test',
        plan: 'test',
        generated_at: '2025-01-01T00:00:00Z',
        model_used: 'test',
      });

    const { result } = renderHook(() => useSoapNote());

    // First call fails
    await act(async () => {
      await result.current.generateSoapNote('Test 1');
    });
    expect(result.current.soapError).toBe('First error');

    // Second call succeeds and clears error
    await act(async () => {
      await result.current.generateSoapNote('Test 2');
    });
    expect(result.current.soapError).toBeNull();
  });

  it('can set ollama status directly', () => {
    const { result } = renderHook(() => useSoapNote());

    const status = {
      connected: true,
      available_models: ['qwen3:4b', 'llama3:8b'],
      error: null,
    };

    act(() => {
      result.current.setOllamaStatus(status);
    });

    expect(result.current.ollamaStatus).toEqual(status);
  });

  it('can set ollama models directly', () => {
    const { result } = renderHook(() => useSoapNote());

    const models = ['model1', 'model2', 'model3'];

    act(() => {
      result.current.setOllamaModels(models);
    });

    expect(result.current.ollamaModels).toEqual(models);
  });

  it('can set soap error directly', () => {
    const { result } = renderHook(() => useSoapNote());

    act(() => {
      result.current.setSoapError('Custom error');
    });

    expect(result.current.soapError).toBe('Custom error');

    act(() => {
      result.current.setSoapError(null);
    });

    expect(result.current.soapError).toBeNull();
  });
});

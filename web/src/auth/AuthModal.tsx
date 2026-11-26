import React, { useState } from 'react';
import { useAuth } from './useAuth';
import { DpdpConsents } from '../types';

export function AuthModal() {
  const { isAuthModalOpen, closeAuthModal, sendOtp, verifyOtp } = useAuth();

  const [step, setStep] = useState<'phone' | 'verify'>('phone');
  const [phone, setPhone] = useState('+91');
  const [code, setCode] = useState('');
  const [handle, setHandle] = useState('');
  const [displayName, setDisplayName] = useState('');
  const [ageConfirmed, setAgeConfirmed] = useState(false);
  const [consents, setConsents] = useState<DpdpConsents>({
    essential: true,
    functional: true,
    marketing: false,
    personalised_ads: false,
  });

  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  if (!isAuthModalOpen) return null;

  const handleSendOtp = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);
    const cleanPhone = phone.trim();
    if (!cleanPhone || cleanPhone.length < 10) {
      setError('Please enter a valid phone number with country code (e.g. +919876543210)');
      return;
    }

    setIsSubmitting(true);
    try {
      await sendOtp(cleanPhone);
      setStep('verify');
    } catch (err: any) {
      setError(err?.message || 'Failed to send verification code');
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleVerify = async (e: React.FormEvent) => {
    e.preventDefault();
    setError(null);

    if (!code || code.trim().length < 4) {
      setError('Please enter the 6-digit verification code');
      return;
    }

    if (!handle.trim()) {
      setError('Please choose a player handle');
      return;
    }

    if (!ageConfirmed) {
      setError('You must be 18 years or older to participate in Chess Hustle.');
      return;
    }

    if (!consents.essential) {
      setError('Essential consent is required to process your gameplay and account data.');
      return;
    }

    setIsSubmitting(true);
    try {
      await verifyOtp(
        phone.trim(),
        code.trim(),
        ageConfirmed,
        consents,
        handle.trim().toLowerCase(),
        displayName.trim() || handle.trim()
      );
    } catch (err: any) {
      setError(err?.message || 'Verification failed. Please check the code.');
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <div className="modal-backdrop" role="dialog" aria-modal="true" aria-labelledby="auth-modal-title">
      <div className="modal-card">
        <header className="modal-header">
          <div>
            <h2 id="auth-modal-title" className="modal-title">
              {step === 'phone' ? 'Sign in to Chess Hustle' : 'Verify Phone & Setup Profile'}
            </h2>
            <p className="modal-sub">
              {step === 'phone'
                ? 'Enter your WhatsApp number to receive a one-time login code.'
                : `Enter the WhatsApp code sent to ${phone} and configure your privacy settings.`}
            </p>
          </div>
          <button
            type="button"
            className="modal-close"
            onClick={closeAuthModal}
            aria-label="Close modal"
          >
            ✕
          </button>
        </header>

        {error && (
          <div className="alert alert-error" role="alert">
            <span>{error}</span>
          </div>
        )}

        {step === 'phone' ? (
          <form onSubmit={handleSendOtp} className="modal-form">
            <div className="form-group">
              <label htmlFor="phone-input" className="form-label">
                Mobile Number
              </label>
              <div className="input-affix-wrapper">
                <input
                  id="phone-input"
                  type="tel"
                  className="form-input"
                  placeholder="+919876543210"
                  value={phone}
                  onChange={(e) => setPhone(e.target.value)}
                  autoFocus
                  required
                />
              </div>
              <span className="form-hint">
                Include country code (e.g. +91 for India). We will send your 6-digit login code on WhatsApp.
              </span>
            </div>

            <div className="modal-actions">
              <button
                type="submit"
                className="btn btn-primary btn-block"
                disabled={isSubmitting}
              >
                {isSubmitting ? 'Sending code…' : 'Send One-Time Code'}
              </button>
            </div>
          </form>
        ) : (
          <form onSubmit={handleVerify} className="modal-form">
            <div className="form-group">
              <label htmlFor="otp-input" className="form-label">
                One-Time Password (OTP)
              </label>
              <input
                id="otp-input"
                type="text"
                inputMode="numeric"
                pattern="[0-9]*"
                maxLength={6}
                className="form-input tabular-nums"
                placeholder="482913"
                value={code}
                onChange={(e) => setCode(e.target.value)}
                autoFocus
                required
              />
            </div>

            <div className="form-row">
              <div className="form-group">
                <label htmlFor="handle-input" className="form-label">
                  Handle (Unique Username)
                </label>
                <input
                  id="handle-input"
                  type="text"
                  className="form-input"
                  placeholder="ananya_plays"
                  value={handle}
                  onChange={(e) => setHandle(e.target.value.replace(/[^a-zA-Z0-9_]/g, ''))}
                  required
                />
              </div>
              <div className="form-group">
                <label htmlFor="name-input" className="form-label">
                  Display Name
                </label>
                <input
                  id="name-input"
                  type="text"
                  className="form-input"
                  placeholder="Ananya"
                  value={displayName}
                  onChange={(e) => setDisplayName(e.target.value)}
                />
              </div>
            </div>

            <div className="compliance-section">
              <h3 className="section-title">Age Declaration & Privacy Consents</h3>

              {/* 18+ Age Gate Declaration (ADR-010) */}
              <label className="checkbox-item age-gate">
                <input
                  type="checkbox"
                  checked={ageConfirmed}
                  onChange={(e) => setAgeConfirmed(e.target.checked)}
                  required
                />
                <span className="checkbox-text">
                  <strong>I declare and confirm that I am 18 years of age or older.</strong>
                  <span className="checkbox-subtext">
                    Chess Hustle is strictly restricted to adults under DPDP regulations.
                  </span>
                </span>
              </label>

              {/* DPDP Act 2023 Consent Checkboxes */}
              <div className="dpdp-group">
                <label className="checkbox-item">
                  <input
                    type="checkbox"
                    checked={consents.essential}
                    disabled
                    readOnly
                  />
                  <span className="checkbox-text">
                    <strong>Essential gameplay & account processing</strong> (Required)
                    <span className="checkbox-subtext">
                      Necessary for ratings, game rules, and ledger balance tracking.
                    </span>
                  </span>
                </label>

                <label className="checkbox-item">
                  <input
                    type="checkbox"
                    checked={consents.functional}
                    onChange={(e) =>
                      setConsents({ ...consents, functional: e.target.checked })
                    }
                  />
                  <span className="checkbox-text">
                    <strong>Functional preferences</strong>
                    <span className="checkbox-subtext">
                      Stores board cosmetics, sound preferences, and UI settings.
                    </span>
                  </span>
                </label>

                <label className="checkbox-item">
                  <input
                    type="checkbox"
                    checked={consents.marketing}
                    onChange={(e) =>
                      setConsents({ ...consents, marketing: e.target.checked })
                    }
                  />
                  <span className="checkbox-text">
                    <strong>Marketing & updates</strong>
                    <span className="checkbox-subtext">
                      Receive notices about tournaments, leaderboards, and events.
                    </span>
                  </span>
                </label>

                <label className="checkbox-item">
                  <input
                    type="checkbox"
                    checked={consents.personalised_ads}
                    onChange={(e) =>
                      setConsents({ ...consents, personalised_ads: e.target.checked })
                    }
                  />
                  <span className="checkbox-text">
                    <strong>Personalized rewarded ads</strong>
                    <span className="checkbox-subtext">
                      Allows partner advertising networks to personalize rewarded video offers.
                    </span>
                  </span>
                </label>
              </div>
            </div>

            <div className="modal-actions form-row">
              <button
                type="button"
                className="btn btn-secondary"
                onClick={() => setStep('phone')}
                disabled={isSubmitting}
              >
                Back
              </button>
              <button
                type="submit"
                className="btn btn-primary"
                disabled={isSubmitting || !ageConfirmed}
              >
                {isSubmitting ? 'Verifying…' : 'Verify & Play'}
              </button>
            </div>
          </form>
        )}
      </div>
    </div>
  );
}

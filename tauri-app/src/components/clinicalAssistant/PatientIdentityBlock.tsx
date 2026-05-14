import { memo } from 'react';

interface PatientIdentityBlockProps {
  name: string | null;
  dob: string | null;
}

export const PatientIdentityBlock = memo(function PatientIdentityBlock({
  name,
  dob,
}: PatientIdentityBlockProps) {
  const hasIdentity = !!(name || dob);

  return (
    <section className="ca-sidebar-section">
      <h2>Patient</h2>
      {hasIdentity ? (
        <div className="ca-patient-id">
          {name && <div className="ca-patient-id-name">{name}</div>}
          {dob && <div className="ca-patient-id-dob">DOB {dob}</div>}
        </div>
      ) : (
        <div className="ca-patient-id-empty">
          Not detected — click Re-extract below to try again
        </div>
      )}
    </section>
  );
});

export default PatientIdentityBlock;

import { memo } from 'react';
import type { UseMedicationAssessmentResult } from '../../hooks/useMedicationAssessment';
import { PatientContextPanel } from '../medicationAssessment/PatientContextPanel';
import { PatientIdentityBlock } from './PatientIdentityBlock';
import { MedicationsBlock } from './MedicationsBlock';
import { ClinicalContextBlock } from './ClinicalContextBlock';

interface SidebarProps {
  med: UseMedicationAssessmentResult;
}

/**
 * Left rail of the Clinical Assistant window. Persistent "this patient"
 * surface: every tab on the right pane reads from this state.
 */
export const Sidebar = memo(function Sidebar({ med }: SidebarProps) {
  return (
    <aside className="ca-sidebar" aria-label="Patient context sidebar">
      <PatientIdentityBlock name={med.patientName} dob={med.patientDob} />

      <MedicationsBlock med={med} />

      <section className="ca-sidebar-section">
        <h2>Patient Context</h2>
        <PatientContextPanel
          patientAge={med.patientAge}
          patientDob={med.patientDob}
          patientEgfr={med.patientEgfr}
          patientConditions={med.patientConditions}
          strategy={med.strategy}
          setPatientAge={med.setPatientAge}
          setPatientEgfr={med.setPatientEgfr}
          setPatientCondition={med.setPatientCondition}
          setStrategy={med.setStrategy}
        />
      </section>

      <ClinicalContextBlock
        clinicalContext={med.clinicalContext}
        setClinicalContext={med.setClinicalContext}
        extractionState={med.extractionState}
      />
    </aside>
  );
});

export default Sidebar;

import type { BillingConfidence, BillingStatus, CapWarningLevel } from '../../types';

export function formatCents(cents: number): string {
  return `$${(cents / 100).toFixed(2)}`;
}

export function formatHours(hours: number): string {
  return `${hours.toFixed(1)}h`;
}

export function capWarningColor(level: CapWarningLevel): string {
  switch (level) {
    case 'normal': return 'var(--accent-blue, #3b82f6)';
    case 'warning': return 'var(--accent-stopping, #f59e0b)';
    case 'critical':
    case 'exceeded': return 'var(--accent-recording, #ef4444)';
  }
}

export function confidenceBadgeClass(confidence: BillingConfidence): string {
  switch (confidence) {
    case 'high': return 'billing-confidence-high';
    case 'medium': return 'billing-confidence-medium';
    case 'low': return 'billing-confidence-low';
    default: return '';
  }
}

export function statusLabel(status: BillingStatus): string {
  return status === 'confirmed' ? 'Confirmed' : 'Draft';
}

/** OHIP code criteria for tooltip display — helps clinicians verify code applicability */
export const OHIP_CODE_CRITERIA: Record<string, string> = {
  // Assessments
  A001A: 'Minor Assessment: Single focused complaint, brief history + targeted exam, <10 min',
  A003A: 'General Assessment: Comprehensive new patient workup OR annual exam. Multi-system history + full physical, 20-45 min',
  A004A: 'General Re-Assessment: Comprehensive established patient follow-up, multiple active problems, multi-system review, 20-30 min',
  A007A: 'Intermediate Assessment: Moderate complexity, 1-2 issues, 10-20 min. Standard follow-up or well-baby check',
  A008A: 'Mini Assessment: Very brief, <5 min. Single Rx renewal without exam, form signature',
  A888A: 'Weekend/Holiday Special Visit Assessment',
  // Counselling
  K005A: 'Individual Counselling: Per-unit counselling session',
  K013A: 'Extended Counselling: Primarily counselling visit, mental health, lifestyle, substance use',
  K017A: 'Antenatal Preventive Assessment',
  K033A: 'Additional Counselling',
  K130A: 'Periodic Health Visit (18-44): Annual preventive health exam, age-appropriate screening',
  K131A: 'Periodic Health Visit (45-64): Annual preventive health exam',
  K132A: 'Periodic Health Visit (65+): Annual preventive health exam',
  // Procedures (50% shadow)
  G365A: 'Pap Smear: Cervical cytology collected — speculum exam + sample taken',
  G378A: 'IUD Insertion: IUD physically inserted during visit',
  G552A: 'IUD Removal: IUD physically removed during visit',
  R048A: 'Malignant Lesion Excision (<1cm): Suspicious lesion excised, pathology sent',
  R051A: 'Malignant Lesion Excision (1-2cm)',
  R094A: 'Malignant Lesion Excision (>2cm)',
  Z101A: 'Abscess I&D: Incision, drainage, and packing of abscess',
  Z104A: 'Skin Biopsy: Punch/shave biopsy taken for pathology',
  Z108A: 'Cryotherapy (single): Liquid nitrogen to one lesion',
  Z110A: 'Cryotherapy (2-5 lesions)',
  Z112A: 'Electrocoagulation (single lesion)',
  Z113A: 'Electrocoagulation (2-5 lesions)',
  Z114A: 'Benign Excision (<1cm): Lipoma, cyst, or skin tag removed',
  Z119A: 'Benign Excision (1-2cm)',
  Z154A: 'Laceration Repair (<5cm): Simple wound sutured',
  Z160A: 'Laceration Repair (5-10cm)',
  Z176A: 'Complex Laceration Repair: Deep tissue, layered closure',
  Z314A: 'Epistaxis Cautery: Silver nitrate/electrocautery for nosebleed',
  Z315A: 'Epistaxis Packing: Anterior nasal packing inserted',
  Z535A: 'Sigmoidoscopy: Flexible sigmoidoscopy performed in office',
  Z543A: 'Anoscopy: Performed in office',
  Z545A: 'Thrombosed Hemorrhoid Incision: External hemorrhoid drained',
  Z847A: 'Corneal Foreign Body Removal: Removed with slit lamp/needle',
  // Immunizations
  G462A: 'Travel Immunization: Travel-related vaccine administered',
  G538A: 'Immunization: General vaccine administration',
  G840A: 'Influenza Vaccine Administration',
  G841A: 'Pneumococcal Vaccine Administration',
  G842A: 'Hepatitis B Vaccine Administration',
  G843A: 'MMR Vaccine Administration',
  G844A: 'Td/Tdap Vaccine Administration',
  G848A: 'Other Vaccine Administration',
  // Screening
  G590A: 'Colorectal Screening Discussion',
  G591A: 'Breast Screening Discussion',
  E430A: 'Tray Fee (with Pap smear)',
  E431A: 'Tray Fee (with Pap smear)',
  E079A: 'Smoking Cessation Discussion (add-on)',
  // Out-of-basket
  P003A: 'Prenatal General Assessment: FIRST prenatal visit — complete OB history, baseline labs, dating',
  P004A: 'Prenatal Re-Assessment: Follow-up prenatal — fundal height, FHR, BP',
  P005A: 'Antenatal Preventive Health Assessment',
  K028A: 'STI Management: STI testing ordered/performed, treatment prescribed, or contact tracing',
  K029A: 'Insulin Therapy Support (max 6/year)',
  K023A: 'Palliative Care: Symptom management for terminal/life-limiting illness',
  K039A: 'Smoking Cessation Follow-Up: Check-in on active quit attempt (max 2/year)',
  K032A: 'Neurocognitive Assessment: FORMAL cognitive testing (MMSE, MoCA, clock drawing) — 20+ min. NOT general memory complaints or neurological exam',
  K070A: 'Home Care Application: CCAC/home care referral form submitted',
  K071A: 'Acute Home Care Supervision (first 8 weeks)',
  K140A: 'Shared Appointment (2 patients): Group chronic disease education',
  K141A: 'Shared Appointment (3 patients)',
  K142A: 'Shared Appointment (4 patients)',
  K143A: 'Shared Appointment (5 patients)',
  K144A: 'Shared Appointment (6+ patients)',
  Q040A: 'Diabetes Management Incentive: After 3+ K030A visits/year — active A1C review, med adjustment',
  Q042A: 'Smoking Cessation Fee: Counselling provided — quit date, NRT, triggers discussed',
  Q050A: 'CHF Management Incentive: Active fluid status, diuretic adjustment, weight monitoring',
  Q012A: 'After-Hours Premium: 50% premium for eligible services outside clinic hours',
  Q053A: 'Patient Attachment Bonus: $500 for newly rostered patient',
  Q054A: 'Mother & Newborn Bonus: $350',
  // Time-based
  Q310: 'Direct Patient Care: In-person, video, or phone-from-office encounters. $80/hr ($20/15min)',
  Q311: 'Telephone Remote Care: Phone calls when physician is NOT in clinic. $68/hr ($17/15min)',
  Q312: 'Indirect Patient Care: Charting, lab review, referral letters, care coordination. $80/hr ($20/15min)',
  Q313: 'Clinical Administration: Screening programs, EMR updates, QI initiatives. $80/hr ($20/15min)',
};

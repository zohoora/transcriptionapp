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
  // Injections & joint procedures
  G373A: 'Injection — Sole Reason for Visit: IM, SC, or joint injection with no other assessment',
  G369A: 'Epidural Injection: Caudal or lumbar epidural steroid injection performed',
  G370A: 'Nerve Block: Peripheral nerve block performed (e.g., digital, intercostal)',
  G371A: 'Trigger Point Injection (single): Single-site trigger point injection',
  G372A: 'Trigger Point Injection (multiple): Multi-site trigger point injections',
  Z331A: 'Joint Injection (small): Intra-articular injection — small joint (finger, wrist, ankle)',
  Z332A: 'Joint Injection (large): Intra-articular injection — large joint (knee, shoulder, hip)',
  G394A: 'Pap Smear Repeat: Follow-up or repeat Pap smear collection',
  // Additional procedures
  Z117A: 'Wound Debridement: Wound care with debridement of devitalized tissue',
  Z129A: 'Foreign Body Removal (skin): Removal of foreign body from skin or subcutaneous tissue',
  Z169A: 'Toenail Removal: Partial or complete toenail removal (ingrown nail, onychocryptosis)',
  E502A: 'Tray Fee — Minor Procedure: Disposable supplies for minor office procedure',
  K030A: 'Diabetic Management Assessment: Active diabetes care visit — A1C review, med adjustment, foot exam (max 4/year)',
  A900A: 'Telephone/Email Management: Brief phone/email patient management per call',
  K022A: 'Abnormal Pap Counselling: Counselling for abnormal Papanicolaou smear result',
  // Time-based
  Q310: 'Direct Patient Care: In-person, video, or phone-from-office encounters. $80/hr ($20/15min)',
  Q311: 'Telephone Remote Care: Phone calls when physician is NOT in clinic. $68/hr ($17/15min)',
  Q312: 'Indirect Patient Care: Charting, lab review, referral letters, care coordination. $80/hr ($20/15min)',
  Q313: 'Clinical Administration: Screening programs, EMR updates, QI initiatives. $80/hr ($20/15min)',
  // Telehealth / Virtual Care
  B960A: 'Telephone Intermediate Assessment: Phone-based assessment, moderate complexity, 1-2 issues',
  B961A: 'Telephone Minor Assessment: Phone-based single focused complaint, brief',
  B962A: 'Telephone General/Complete Assessment: Phone-based comprehensive assessment, multi-system',
  K083A: 'Telephone/Email Clinical Management: Per 15 min clinical management via phone or email',
  K082A: 'Telehealth Consultation Fee: Telehealth-delivered consultation',
  // Mental Health
  K007A: 'Psychotherapy — Individual (half hour+): Individual psychotherapy session, 30+ minutes',
  K002A: 'Individual Psychotherapy (half hour): Individual psychotherapy session, ~30 minutes',
  K197A: 'Prenatal Genetic Counselling: Genetic counselling for prenatal patients',
  // Hospital Visits
  C003A: 'Hospital Admission Assessment: Full admission assessment for hospitalized patient',
  C004A: 'Hospital Admission — Partial: Partial admission assessment (e.g., already assessed by other MD)',
  C009A: 'Hospital Subsequent Visit: Follow-up visit for hospitalized patient',
  C010A: 'Hospital Concurrent Care: Concurrent care visit when another physician is MRP',
  C012A: 'Hospital Discharge Day Management: Discharge planning and management on day of discharge',
  C001A: 'Family Practice Consultation: In-office consultation requested by another physician',
  C002A: 'Repeat Consultation: Follow-up consultation for previously consulted patient',
  H003A: 'Newborn Hospital Care — First Day: Initial newborn assessment and care in hospital',
  H004A: 'Newborn Hospital Care — Subsequent: Follow-up newborn care in hospital, per day',
  // Long-Term Care
  A191A: 'LTC New Admission: Comprehensive assessment for newly admitted LTC resident',
  A192A: 'LTC Subsequent Visit: Follow-up visit for LTC resident',
  A193A: 'LTC Annual Comprehensive: Annual comprehensive assessment for existing LTC resident',
  A194A: 'LTC Intermediate Visit: Intermediate complexity visit for LTC resident',
  A195A: 'LTC Pronouncement of Death: Pronouncement of death for LTC resident',
  // House Calls
  A901A: 'House Call Assessment: Assessment performed at patient\'s home',
  A902A: 'House Call — Pronouncement of Death: Home visit for pronouncement of death',
  A903A: 'House Call — Additional Patient: Additional patient at same residence during house call',
  // Prenatal / Obstetric
  P001A: 'Prenatal Visit — First: Initial prenatal visit, complete OB history + baseline',
  P002A: 'Prenatal Visit — Subsequent: Follow-up prenatal visit',
  P006A: 'Prenatal Global: Global fee covering all prenatal visits (mutually exclusive with P001A/P002A)',
  P007A: 'Postnatal Visit — General: Postnatal general assessment',
  P008A: 'Postnatal Visit — Subsequent: Follow-up postnatal visit',
  P009A: 'Prenatal Care — Late Transfer In: Prenatal care for late-transfer patient',
  P018A: 'Postpartum Care — Comprehensive: Comprehensive postpartum assessment',
  P013A: 'Labour Management — First 2 Hours: Active labour management, initial 2 hours',
  P014A: 'Labour Management — Additional Hour: Each additional hour of active labour management',
  // Palliative Care
  K036A: 'Palliative Care Counselling (office): Half hour+ palliative care counselling in office',
  K037A: 'Palliative Care Counselling — Subsequent: Follow-up palliative counselling',
  K038A: 'Palliative Care — Home Visit: Palliative care counselling at patient\'s home',
  E082A: 'Palliative Care Premium: Add-on premium for palliative care visit',
  B998A: 'Home Palliative Phone Management: Per 15 min phone management for palliative patient at home',
  // Geriatric
  K655A: 'Comprehensive Geriatric Assessment: Annual comprehensive assessment for patients 75+',
  K656A: 'Geriatric Assessment — Follow-Up: Follow-up to comprehensive geriatric assessment',
  // Preventive Care Bonuses
  Q010A: 'Childhood Immunization Bonus: Per completed immunization series',
  Q015A: 'Flu Immunization Bonus: Bonus for administering influenza vaccine',
  Q100A: 'Cervical Screening Bonus: Bonus for Pap/cervical screening referral',
  Q101A: 'Mammography Screening Bonus: Bonus for mammography screening referral',
  Q102A: 'Colorectal Cancer Screening Bonus: Bonus for FOBT/FIT screening referral',
  Q200A: 'New Patient Intake Incentive: Incentive for rostering a new patient',
  // Form Completion
  K031A: 'Certificate — Short: Sick note, return-to-work, brief certificate',
  K035A: 'Certificate — Long: Insurance report, disability report, detailed certificate',
  K034A: 'Transfer of Care Summary: Transfer of care report or summary letter',
  // Additional Procedures
  G420A: 'Ear Syringing: Cerumen removal (bilateral ear irrigation)',
  G313A: 'Aspiration — Abscess/Cyst/Hematoma: Needle aspiration of abscess, cyst, or hematoma',
  Z200A: 'Curette — Skin Lesion: Shave or curettage of skin lesion',
  Z201A: 'Curette — Additional Lesion: Each additional skin lesion curetted',
  E540A: 'Toenail Removal — Under Block: Toenail removal with local anesthetic block',
  E541A: 'Toenail Wedge Resection with Phenol: Wedge resection with phenol matrixectomy',
  // eConsult
  K998A: 'Physician-to-Physician Telephone Consultation: Phone consultation between physicians',
  K738A: 'eConsult — Specialist Seeking GP Input: Electronic consultation from specialist',
};

/** Exclusion group — codes within the same group cannot be billed together */
interface ExclusionGroup {
  name: string;
  codes: string[];
  reason: string;
}

const EXCLUSION_GROUPS: ExclusionGroup[] = [
  { name: 'Core assessments', codes: ['A001A', 'A003A', 'A004A', 'A007A', 'A008A', 'A888A'], reason: 'Only one assessment code per encounter' },
  { name: 'Periodic health visits', codes: ['K130A', 'K131A', 'K132A'], reason: 'One age-band periodic health visit per encounter' },
  { name: 'Assessment vs periodic', codes: ['A001A', 'A003A', 'A004A', 'A007A', 'A008A', 'A888A', 'K130A', 'K131A', 'K132A'], reason: 'Assessment and periodic health visit are mutually exclusive' },
  { name: 'Counselling codes', codes: ['K005A', 'K013A', 'K033A'], reason: 'One counselling code per encounter' },
  { name: 'Prenatal codes', codes: ['P003A', 'P004A', 'P005A'], reason: 'One prenatal assessment type per encounter' },
  { name: 'Prenatal vs assessment', codes: ['P003A', 'P004A', 'P005A', 'A001A', 'A003A', 'A004A', 'A007A', 'A008A', 'A888A'], reason: 'Prenatal assessment replaces standard assessment' },
  { name: 'Malignant excision sizes', codes: ['R048A', 'R051A', 'R094A'], reason: 'One excision size category per lesion' },
  { name: 'Benign excision sizes', codes: ['Z114A', 'Z119A'], reason: 'One excision size category per lesion' },
  { name: 'Laceration repair sizes', codes: ['Z154A', 'Z160A', 'Z176A'], reason: 'One complexity level per wound' },
  { name: 'Cryotherapy single/multiple', codes: ['Z108A', 'Z110A'], reason: 'Single vs multiple lesion — pick one' },
  { name: 'Electrocoagulation single/multiple', codes: ['Z112A', 'Z113A'], reason: 'Single vs multiple lesion — pick one' },
  { name: 'Epistaxis treatment', codes: ['Z314A', 'Z315A'], reason: 'Cautery vs packing — typically one per encounter' },
  { name: 'Direct care time', codes: ['Q310', 'Q311'], reason: 'In-office vs remote — one setting per encounter' },
  { name: 'Trigger point single/multiple', codes: ['G371A', 'G372A'], reason: 'Single vs multiple sites — pick one' },
  { name: 'Joint injection size', codes: ['Z331A', 'Z332A'], reason: 'Small vs large joint — pick one per joint' },
  { name: 'Prenatal visit types', codes: ['P001A', 'P006A'], reason: 'Individual visits vs global — can\'t bill both' },
  { name: 'Hospital assessment types', codes: ['C003A', 'C004A'], reason: 'Full vs partial admission assessment' },
  { name: 'Palliative counselling', codes: ['K036A', 'K037A'], reason: 'Initial vs subsequent per visit' },
  { name: 'LTC assessment types', codes: ['A191A', 'A193A'], reason: 'Admission vs annual — different purposes but both comprehensive' },
];

/** Check if a new code conflicts with any existing codes */
export function findConflicts(existingCodes: string[], newCode: string): Array<{ code: string; reason: string }> {
  const results: Array<{ code: string; reason: string }> = [];
  for (const group of EXCLUSION_GROUPS) {
    if (!group.codes.includes(newCode)) continue;
    for (const existing of existingCodes) {
      if (existing === newCode) continue;
      if (group.codes.includes(existing) && !results.some(r => r.code === existing)) {
        results.push({ code: existing, reason: group.reason });
      }
    }
  }
  return results;
}

/** Get all conflicts among a set of codes (returns map of code → conflicting codes) */
export function findAllConflicts(codes: string[]): Map<string, Array<{ code: string; reason: string }>> {
  const map = new Map<string, Array<{ code: string; reason: string }>>();
  for (let i = 0; i < codes.length; i++) {
    const others = codes.filter((_, j) => j !== i);
    const conflicts = findConflicts(others, codes[i]);
    if (conflicts.length > 0) {
      map.set(codes[i], conflicts);
    }
  }
  return map;
}

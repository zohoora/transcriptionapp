import type { BillingConfidence, BillingStatus, CapWarningLevel } from '../../types';

/**
 * Base → Add-on code pairs. When the user increases quantity on a base code,
 * the base stays at 1 and the add-on code is added/incremented instead.
 */
export const ADDON_CODE_PAIRS: Record<string, { addonCode: string; maxAddonQty: number }> = {
  'G370A': { addonCode: 'G371A', maxAddonQty: 5 },  // Joint injection → additional joints
  'G384A': { addonCode: 'G385A', maxAddonQty: 2 },  // Trigger point → additional sites
  'G231A': { addonCode: 'G223A', maxAddonQty: 5 },  // Nerve block peripheral → additional
  'G373A': { addonCode: 'G372A', maxAddonQty: 5 },  // IM injection sole → additional with visit
};

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

/** OHIP code criteria for tooltip display — helps clinicians verify code applicability.
 *  Generated from April 2026 Schedule of Benefits. 190 codes total. */
export const OHIP_CODE_CRITERIA: Record<string, string> = {
  // ═══════════════════════════════════════════════════════════════════════
  // IN-BASKET (136 codes)
  // ═══════════════════════════════════════════════════════════════════════

  // Assessments
  A001A: 'Minor Assessment: Single focused complaint, brief history + targeted exam, <10 min',
  A003A: 'General Assessment: Comprehensive new patient workup OR annual exam. Multi-system history + full physical, 20-45 min',
  A004A: 'General Re-Assessment: Comprehensive established patient follow-up, multiple active problems, multi-system review, 20-30 min',
  A007A: 'Intermediate Assessment or Well Baby Care: Moderate complexity, 1-2 issues, 10-20 min',
  A008A: 'Mini Assessment: Very brief, <5 min. Single Rx renewal without exam, form signature',
  A101A: 'Limited Virtual Care -- Video: Video-based virtual care encounter',
  A102A: 'Limited Virtual Care -- Telephone: Phone-based virtual care encounter',
  A110A: 'Periodic Oculo-Visual Assessment (19 and below): Eye exam for children/adolescents',
  A112A: 'Periodic Oculo-Visual Assessment (65 and above): Eye exam for seniors',
  A777A: 'Intermediate Assessment -- Pronouncement of Death',
  A900A: 'Complex House Call Assessment: Assessment for frail/housebound patients',

  // FPA Assessments
  A917A: 'FPA -- Sport Medicine: Requires Sport Medicine FPA designation',
  A927A: 'FPA -- Allergy: Requires Allergy FPA designation',
  A937A: 'FPA -- Pain Management: Requires Pain Management FPA designation',
  A947A: 'FPA -- Sleep Medicine: Requires Sleep Medicine FPA designation',
  A957A: 'FPA -- Addiction Medicine: Requires Addiction Medicine FPA designation',
  A967A: 'FPA -- Care of the Elderly: Requires Care of the Elderly FPA designation',

  // SVP -- Office
  A990A: 'SVP Office -- Weekday Daytime (07:00-17:00)',
  A994A: 'SVP Office -- Evening (17:00-24:00) Mon-Fri',
  A996A: 'SVP Office -- Night (00:00-07:00)',
  A998A: 'SVP Office -- Sat/Sun/Holiday (07:00-24:00)',

  // SVP -- Home
  B990A: 'SVP Home -- Weekday Non-elective/Elective',
  B992A: 'SVP Home -- Weekday with Sacrifice of Office Hours',
  B993A: 'SVP Home -- Sat/Sun/Holiday (07:00-24:00)',
  B994A: 'SVP Home -- Evening (17:00-24:00) Mon-Fri',
  B996A: 'SVP Home -- Night (00:00-07:00)',

  // Hospital
  C882A: 'Hospital Palliative Care -- MRP Subsequent Visit',

  // Tray Fee
  E542A: 'Tray Fee -- Procedure Outside Hospital',

  // Lab
  G001A: 'Lab -- Cholesterol, Total',
  G002A: 'Lab -- Glucose, Quantitative/Semi-Quantitative',
  G004A: 'Lab -- Occult Blood',
  G005A: 'Lab -- Pregnancy Test',
  G009A: 'Lab -- Urinalysis, Routine (includes microscopy)',
  G010A: 'Lab -- Urinalysis Without Microscopy',
  G011A: 'Lab -- Fungus Culture incl KOH Prep',
  G012A: 'Lab -- Wet Preparation (fungus, trichomonas)',
  G014A: 'Lab -- Rapid Streptococcal Test',

  // Nerve Blocks
  G123A: 'Nerve Block -- Obturator, Each Additional (max 4)',
  G223A: 'Nerve Block -- Somatic/Peripheral, Additional Sites',
  G227A: 'Nerve Block -- Other Cranial Nerve',
  G228A: 'Nerve Block -- Paravertebral (cervical/thoracic/lumbar/sacral)',
  G231A: 'Nerve Block -- Somatic/Peripheral, One Nerve or Site',
  G235A: 'Nerve Block -- Supraorbital',

  // Allergy
  G197A: 'Allergy Skin Testing -- Professional Component (max 50/yr)',
  G202A: 'Allergy -- Hyposensitisation, Each Injection',
  G205A: 'Allergy -- Insect Venom Desensitisation (max 5/day)',
  G209A: 'Allergy Skin Testing -- Technical Component (max 50/yr)',
  G212A: 'Allergy -- Hyposensitisation, Sole Reason for Visit',

  // Cardiovascular
  G271A: 'Anticoagulant Supervision -- Long-Term, Phone/Month',

  // ECG
  G310A: 'ECG Twelve Lead -- Technical Component',
  G313A: 'ECG Twelve Lead -- Professional Component (written interp)',

  // Gynaecology
  G365A: 'Papanicolaou Smear -- Periodic: Cervical cytology collected',
  G378A: 'IUD Insertion: IUD physically inserted during visit',
  G394A: 'Papanicolaou Smear -- Additional/Repeat',
  G552A: 'IUD Removal: IUD physically removed during visit',

  // Injections/Infusions
  G370A: 'Injection/Aspiration of Joint, Bursa, Ganglion, Tendon Sheath',
  G371A: 'Additional Joint/Bursa/Ganglion/Tendon Sheath (add-on to G370, max 5)',
  G372A: 'IM/SC/Intradermal -- Each Additional Injection (with visit)',
  G373A: 'IM/SC/Intradermal -- Sole Reason (first injection)',
  G375A: 'Intralesional Infiltration -- 1 or 2 Lesions',
  G377A: 'Intralesional Infiltration -- 3 or More Lesions',
  G379A: 'Intravenous -- Child, Adolescent or Adult',
  G381A: 'Chemotherapy -- Standard Agents, Minor Toxicity',
  G384A: 'Trigger Point Injection -- Infiltration of Tissue',
  G385A: 'Trigger Point -- Each Additional Site (max 2)',

  // Other D&T
  G420A: 'Ear Syringing/Curetting -- Unilateral or Bilateral',
  G435A: 'Tonometry',
  G462A: 'Administration of Oral Polio Vaccine',

  // Lab/Venipuncture
  G481A: 'Haemoglobin Screen and/or Haematocrit',
  G482A: 'Venipuncture -- Child',
  G489A: 'Venipuncture -- Adolescent or Adult',

  // Audiometry
  G525A: 'Pure Tone Audiometry -- Professional Component',

  // Immunizations
  G538A: 'Immunization -- Other Agents',
  G840A: 'Immunization -- DTaP/IPV (paediatric)',
  G841A: 'Immunization -- DTaP-IPV-Hib (paediatric)',
  G842A: 'Immunization -- Hepatitis B',
  G843A: 'Immunization -- HPV',
  G844A: 'Immunization -- Meningococcal C Conjugate',
  G845A: 'Immunization -- MMR',
  G846A: 'Immunization -- Pneumococcal Conjugate',
  G847A: 'Immunization -- Tdap (adult)',
  G848A: 'Immunization -- Varicella',

  // Spirometry
  J324A: 'Spirometry -- Repeat After Bronchodilator',
  J327A: 'Flow Volume Loop -- Repeat After Bronchodilator',

  // Counselling/Mental Health
  K001A: 'Detention -- Per Full Quarter Hour',
  K002A: 'Interviews with Relatives/Authorized Decision-Maker (per unit)',
  K003A: 'Interviews with CAS/Legal Guardian (per unit)',
  K004A: 'Psychotherapy -- Family (2+ members, per unit)',
  K005A: 'Primary Mental Health Care -- Individual (per unit)',
  K006A: 'Hypnotherapy -- Individual (per unit)',
  K007A: 'Psychotherapy -- Individual (per unit)',
  K008A: 'Diagnostic Interview/Counselling -- Child/Parent (per unit)',
  K013A: 'Counselling -- Individual (max 3 units/year, per unit). After limit exhausted, use K033A instead',
  K015A: 'Counselling of Relatives -- Terminally Ill Patient (per unit)',
  K017A: 'Periodic Health Visit -- Child',

  // Home Care
  K070A: 'Completion of Home Care Referral Form',
  K071A: 'Acute Home Care Supervision (first 8 weeks)',

  // Periodic Health Visits
  K130A: 'Periodic Health Visit -- Adolescent: Annual preventive health exam',
  K131A: 'Periodic Health Visit -- Adult 18-64: Annual preventive health exam',
  K132A: 'Periodic Health Visit -- Adult 65+: Annual preventive health exam',
  K133A: 'Periodic Health Visit -- Adult with IDD: Annual preventive health exam',

  // Case Conference/Phone Consult
  K700A: 'Palliative Care Out-Patient Case Conference (per unit)',
  K702A: 'Bariatric Out-Patient Case Conference (per unit)',
  K730A: 'Physician-to-Physician Phone Consultation -- Referring',
  K731A: 'Physician-to-Physician Phone Consultation -- Consultant',
  K732A: 'CritiCall Phone Consultation -- Referring',
  K733A: 'CritiCall Phone Consultation -- Consultant',

  // Integumentary Surgery
  R048A: 'Malignant Lesion Excision -- Face/Neck, Single',
  R094A: 'Malignant Lesion Excision -- Other Areas, Single',
  Z101A: 'Abscess/Haematoma Incision -- Subcutaneous, One',
  Z110A: 'Onychogryphotic Nail -- Extensive Debridement',
  Z113A: 'Biopsy -- Any Method, Without Sutures',
  Z114A: 'Foreign Body Removal -- Local Anaesthetic',
  Z116A: 'Biopsy -- Any Method, With Sutures',
  Z117A: 'Chemical/Cryotherapy Treatment -- One or More Lesions',
  Z122A: 'Group 3 Excision (cyst/lipoma) -- Face/Neck, Single',
  Z125A: 'Group 3 Excision (cyst/lipoma) -- Other Areas, Single',
  Z128A: 'Nail Plate Excision Requiring Anaesthesia -- One',
  Z129A: 'Nail Plate Excision -- Multiple',
  Z154A: 'Laceration Repair -- Up to 5cm (face/layers)',
  Z156A: 'Group 1 Excision (keratosis) -- Excision & Suture, Single',
  Z157A: 'Group 1 Excision (keratosis) -- Excision & Suture, Two',
  Z158A: 'Group 1 Excision (keratosis) -- Excision & Suture, Three+',
  Z159A: 'Group 1 -- Electrocoagulation/Curetting, Single',
  Z160A: 'Group 1 -- Electrocoagulation/Curetting, Two',
  Z161A: 'Group 1 -- Electrocoagulation/Curetting, Three+',
  Z162A: 'Group 2 (nevus) -- Excision & Suture, Single',
  Z175A: 'Laceration Repair -- 5.1 to 10cm',
  Z176A: 'Laceration Repair -- Up to 5cm (simple)',
  Z314A: 'Epistaxis -- Cauterization, Unilateral',
  Z315A: 'Epistaxis -- Anterior Packing, Unilateral',

  // GI/Urological/Eye
  Z535A: 'Sigmoidoscopy -- Rigid Scope',
  Z543A: 'Anoscopy (Proctoscopy)',
  Z545A: 'Thrombosed Haemorrhoid(s) Incision',
  Z611A: 'Catheterization -- Hospital',
  Z847A: 'Corneal Foreign Body Removal -- One',

  // ═══════════════════════════════════════════════════════════════════════
  // OUT-OF-BASKET (54 codes)
  // ═══════════════════════════════════════════════════════════════════════

  // Consultations
  A005A: 'Consultation: Formal consultation requested by another physician',
  A006A: 'Repeat Consultation: Follow-up consultation for previously consulted patient',
  A888A: 'Emergency Department Equivalent -- Partial Assessment',
  A905A: 'Limited Consultation: Shorter consultation when full consultation not required',

  // Hospital Visits
  C002A: 'Hospital Subsequent Visit -- First Five Weeks',
  C003A: 'Hospital Admission Assessment: Full admission assessment',
  C004A: 'Hospital General Re-Assessment',
  C009A: 'Hospital Subsequent Visit -- After Thirteenth Week',
  C010A: 'Hospital Supportive Care',
  C012A: 'Hospital Discharge Day Management',

  // Premiums/Screening
  E079A: 'Smoking Cessation -- Initial Discussion (add-on)',
  E430A: 'Pap Tray Fee (with G365)',
  E431A: 'Pap Tray Fee -- Immunocompromised',

  // Influenza (FHN only)
  G590A: 'Immunization -- Influenza (FHN only, not FHO basket)',

  // Chronic Disease/Counselling
  K022A: 'HIV Primary Care (per unit)',
  K023A: 'Palliative Care Support (per unit)',
  K028A: 'STI Management (per unit): STI testing/treatment/contact tracing',
  K029A: 'Insulin Therapy Support (per unit, max 6/year)',
  K030A: 'Diabetic Management Assessment: A1C review, med adjustment, foot exam (max 4/year)',
  K031A: 'Form 1 -- Physician Report (Mental Health Act)',
  K032A: 'Specific Neurocognitive Assessment (min 20 min): FORMAL cognitive testing (MMSE, MoCA)',
  K033A: 'Counselling -- Additional Units (per unit)',
  K034A: 'Telephone Reporting -- Specified Reportable Disease to MOH',
  K035A: 'Mandatory Reporting -- Medical Condition to Ontario MOT',
  K036A: 'Northern Health Travel Grant Application Form',
  K037A: 'Fibromyalgia/Myalgic Encephalomyelitis Care (per unit)',
  K038A: 'Completion of LTC Health Assessment Form',
  K039A: 'Smoking Cessation Follow-Up Visit (max 2/year)',

  // Shared Appointments
  K140A: 'Shared Medical Appointment -- 2 Patients',
  K141A: 'Shared Medical Appointment -- 3 Patients',
  K142A: 'Shared Medical Appointment -- 4 Patients',
  K143A: 'Shared Medical Appointment -- 5 Patients',
  K144A: 'Shared Medical Appointment -- 6-12 Patients',

  // eConsult
  K738A: 'Physician-to-Physician eConsult -- Referring',

  // Prenatal/Obstetric
  P001A: 'Attendance at Labour and Delivery (normal)',
  P002A: 'High Risk Prenatal Assessment',
  P003A: 'General Assessment (Major Prenatal Visit): Complete OB history, baseline labs, dating',
  P004A: 'Minor Prenatal Assessment: Follow-up prenatal -- fundal height, FHR, BP',
  P005A: 'Antenatal Preventive Health Assessment',
  P006A: 'Vaginal Delivery: Full labour and delivery',
  P007A: 'Postnatal Care -- Hospital and/or Home',
  P008A: 'Postnatal Care -- Office',
  P009A: 'Attendance at Labour and Delivery (complicated)',
  P018A: 'Caesarean Section',

  // Incentives/Premiums
  Q012A: 'After-Hours Premium: 50% premium for eligible services outside clinic hours',
  Q040A: 'Diabetes Management Incentive: After 3+ K030A visits/year',
  Q042A: 'Smoking Cessation Counselling Fee',
  Q050A: 'Heart Failure Management Incentive',
  Q053A: 'HCC Complex/Vulnerable Patient Bonus: $350',
  Q200A: 'Per Patient Rostering Fee',
  Q888A: 'Weekend Office Access Premium (FHO): Cannot bill with A888A same day',

  // Time-Based
  Q310A: 'Direct Patient Care: In-person, video, or phone-from-office. $80/hr ($20/15min)',
  Q311A: 'Telephone Care -- Not in Office: Phone calls when NOT in clinic. $68/hr ($17/15min)',
  Q312A: 'Indirect Patient Care: Charting, lab review, referral letters. $80/hr ($20/15min)',
  Q313A: 'Clinical Administration: Screening programs, EMR updates, QI. $80/hr ($20/15min)',
};

/** Exclusion group — codes within the same group cannot be billed together */
interface ExclusionGroup {
  name: string;
  codes: string[];
  reason: string;
}

const EXCLUSION_GROUPS: ExclusionGroup[] = [
  { name: 'Core assessments', codes: ['A001A', 'A003A', 'A004A', 'A007A', 'A008A'], reason: 'Only one assessment code per visit' },
  { name: 'Periodic health visits', codes: ['K130A', 'K131A', 'K132A', 'K133A'], reason: 'One periodic health visit per 12 months' },
  { name: 'Assessment vs periodic', codes: ['A001A', 'A003A', 'A004A', 'A007A', 'A008A', 'K130A', 'K131A', 'K132A', 'K133A'], reason: 'Assessment and periodic health visit are mutually exclusive' },
  { name: 'K013 standalone', codes: ['K013A', 'A001A', 'A003A', 'A004A', 'A007A', 'A008A'], reason: 'K013 counselling cannot be billed with assessment codes' },
  { name: 'Prenatal codes', codes: ['P003A', 'P004A', 'P005A'], reason: 'One prenatal assessment type per visit' },
  { name: 'Consultation types', codes: ['A005A', 'A006A', 'A905A'], reason: 'One consultation type per visit' },
  { name: 'Malignant excision', codes: ['R048A', 'R094A'], reason: 'One excision code per lesion' },
  { name: 'Laceration repair sizes', codes: ['Z154A', 'Z175A', 'Z176A'], reason: 'One repair code per wound' },
  { name: 'Group 1 excision vs electro', codes: ['Z156A', 'Z157A', 'Z158A', 'Z159A', 'Z160A', 'Z161A'], reason: 'Pick excision & suture OR electrocoagulation method -- not both' },
  { name: 'Epistaxis treatment', codes: ['Z314A', 'Z315A'], reason: 'Cautery vs packing -- one per encounter' },
  { name: 'Intralesional infiltration count', codes: ['G375A', 'G377A'], reason: '1-2 lesions vs 3+ lesions -- pick one based on count' },
  { name: 'Biopsy method', codes: ['Z113A', 'Z116A'], reason: 'Without sutures vs with sutures -- pick one based on method' },
  { name: 'Nail excision count', codes: ['Z128A', 'Z129A'], reason: 'One nail vs multiple nails -- pick one based on count' },
  { name: 'Group 3 excision location', codes: ['Z122A', 'Z125A'], reason: 'Face/neck vs other areas -- pick one location per lesion' },
  // NOTE: G384A + G385A are base+add-on pair -- NOT mutually exclusive
  // NOTE: G370A + G371A are base+add-on pair -- NOT mutually exclusive
  { name: 'Direct care time', codes: ['Q310A', 'Q311A'], reason: 'In-office vs remote -- one setting per encounter' },
  { name: 'FHO weekend access', codes: ['Q888A', 'A888A'], reason: 'Q888A and A888A cannot be billed same day' },
  { name: 'SVP office premiums', codes: ['A990A', 'A994A', 'A996A', 'A998A'], reason: 'One SVP office premium per visit' },
  { name: 'SVP home premiums', codes: ['B990A', 'B992A', 'B993A', 'B994A', 'B996A'], reason: 'One SVP home premium per visit' },
  { name: 'Prenatal visit types', codes: ['P001A', 'P006A'], reason: 'Individual visits vs global -- can\'t bill both' },
  { name: 'Hospital assessment types', codes: ['C003A', 'C004A'], reason: 'Full vs partial admission assessment' },
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

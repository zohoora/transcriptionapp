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
  // ═══════════════════════════════════════════════════════════════════════
  // IN-BASKET — Assessments
  // ═══════════════════════════════════════════════════════════════════════
  A001A: 'Minor Assessment: Single focused complaint, brief history + targeted exam, <10 min',
  A003A: 'General Assessment: Comprehensive new patient workup OR annual exam. Multi-system history + full physical, 20-45 min',
  A004A: 'General Re-Assessment: Comprehensive established patient follow-up, multiple active problems, multi-system review, 20-30 min',
  A007A: 'Intermediate Assessment / Well Baby Care: Moderate complexity, 1-2 issues, 10-20 min. Standard follow-up or well-baby check',
  A008A: 'Mini Assessment: Very brief, <5 min. Single Rx renewal without exam, form signature',
  A101A: 'Limited Virtual Care Service — Video: Video-based virtual care encounter',
  A102A: 'Limited Virtual Care Service — Phone: Phone-based virtual care encounter',
  A110A: 'Periodic Oculo-Visual Assessment (19 and below): Eye exam for children/adolescents',
  A112A: 'Periodic Oculo-Visual Assessment (65 and above): Eye exam for seniors',
  A777A: 'Intermediate Assessment — Pronouncement of Death (FHO only)',
  A900A: 'Complex House Call Assessment: Assessment for frail/housebound patients (FHO only)',

  // FPA Assessments (require Focused Practice designation)
  A917A: 'FPA — Sport Medicine: Requires Sport Medicine FPA designation',
  A927A: 'FPA — Allergy: Requires Allergy FPA designation',
  A937A: 'FPA — Pain Management: Requires Pain Management FPA designation',
  A947A: 'FPA — Sleep Medicine: Requires Sleep Medicine FPA designation',
  A957A: 'FPA — Addiction Medicine: Requires Addiction Medicine FPA designation',
  A967A: 'FPA — Care of the Elderly: Requires Care of the Elderly FPA designation',

  // SVP — Office
  A990A: 'SVP Office — Weekday Daytime (07:00-17:00): Special visit premium for office daytime',
  A994A: 'SVP Office — Evening (17:00-24:00) Mon-Fri: Evening office premium',
  A996A: 'SVP Office — Night (00:00-07:00): Night office premium',
  A998A: 'SVP Office — Sat/Sun/Holiday (07:00-24:00): Weekend/holiday office premium',

  // SVP — Home (FHO only)
  B990A: 'SVP Home — Weekday Daytime: Home visit premium, weekday daytime',
  B992A: 'SVP Home — Weekday with Sacrifice of Office Hours: Home visit during office hours',
  B993A: 'SVP Home — Sat/Sun/Holiday (07:00-24:00): Home visit weekend/holiday premium',
  B994A: 'SVP Home — Evening (17:00-24:00) Mon-Fri: Home visit evening premium',
  B996A: 'SVP Home — Night (00:00-07:00): Home visit night premium',

  // Hospital (in-basket, FHO only)
  C882A: 'Palliative Care — Subsequent Visits by MRP from ICU Transfer',
  C903A: 'Pre-Dental/Pre-Operative General Assessment',

  // Tray Fee
  E542A: 'Tray Fee — When Procedure Performed Outside Hospital (FHO only)',

  // Lab
  G001A: 'Lab — Cholesterol, Total',
  G002A: 'Lab — Glucose, Quantitative/Semi-Quantitative',
  G004A: 'Lab — Occult Blood',
  G005A: 'Lab — Pregnancy Test',
  G009A: 'Lab — Urinalysis, Routine (includes microscopy)',
  G010A: 'Lab — Urinalysis Without Microscopy',
  G011A: 'Lab — Fungus Culture incl KOH Prep and Smear',
  G012A: 'Lab — Wet Preparation (fungus, trichomonas, parasites)',
  G014A: 'Lab — Rapid Streptococcal Test',

  // Allergy
  G197A: 'Allergy — Skin Testing, Professional Component (max 50/yr)',
  G202A: 'Allergy — Hyposensitisation, Each Injection',
  G205A: 'Allergy — Insect Venom Desensitisation (max 5/day)',
  G209A: 'Allergy — Skin Testing, Technical Component (max 50/yr, FHO only)',
  G212A: 'Allergy — Hyposensitisation, Sole Reason for Visit',

  // Nerve Blocks (FHO only)
  G123A: 'Nerve Block — Obturator, Each Additional (max 4)',
  G223A: 'Nerve Block — Somatic/Peripheral, Additional Nerve(s)',
  G227A: 'Nerve Block — Other Cranial Nerve',
  G228A: 'Nerve Block — Paravertebral (cervical/thoracic/lumbar/sacral/coccygeal)',
  G231A: 'Nerve Block — Somatic/Peripheral, One Nerve or Site',
  G235A: 'Nerve Block — Supraorbital',

  // Cardiovascular
  G271A: 'Anticoagulant Supervision — Long-Term, Telephone/Month',

  // ECG (FHO only)
  G310A: 'ECG — Twelve Lead, Technical Component',
  G313A: 'ECG — Twelve Lead, Professional Component (written interpretation)',

  // Gynaecology
  G365A: 'Papanicolaou Smear — Periodic: Cervical cytology collected — speculum exam + sample taken',
  G378A: 'IUD Insertion: IUD physically inserted during visit (FHO only)',
  G394A: 'Papanicolaou Smear — Additional/Repeat (FHO only)',
  G552A: 'IUD Removal: IUD physically removed during visit (FHO only)',

  // Injections/Infusions
  G370A: 'Injection/Aspiration of Joint, Bursa, Ganglion, or Tendon Sheath (FHO only)',
  G371A: 'Additional Joint/Bursa/Ganglion/Tendon Sheath (add-on to G370, max 5, FHO only)',
  G372A: 'IM/SC/Intradermal — Each Additional Injection (with visit)',
  G373A: 'IM/SC/Intradermal — Sole Reason for Visit (first injection)',
  G375A: 'Intralesional Infiltration — 1 or 2 Lesions',
  G377A: 'Intralesional Infiltration — 3 or More Lesions',
  G379A: 'Intravenous — Child, Adolescent or Adult',
  G381A: 'Chemotherapy — Standard Agents, Minor Toxicity (FHO only)',
  G384A: 'Trigger Point Injection — Infiltration of Tissue',
  G385A: 'Trigger Point — Each Additional Site (add-on to G384, max 2)',

  // Other D&T
  G420A: 'Ear Syringing/Curetting — Unilateral or Bilateral',
  G435A: 'Tonometry',
  G462A: 'Administration of Oral Polio Vaccine',

  // Lab/Venipuncture
  G481A: 'Haemoglobin Screen and/or Haematocrit',
  G482A: 'Venipuncture — Child',
  G489A: 'Venipuncture — Adolescent or Adult',

  // Audiometry
  G525A: 'Pure Tone Threshold Audiometry — Professional Component',

  // Immunizations
  G538A: 'Immunization — Other Agents Not Listed',
  G840A: 'Immunization — DTaP/IPV (paediatric)',
  G841A: 'Immunization — DTaP-IPV-Hib (paediatric)',
  G842A: 'Immunization — Hepatitis B',
  G843A: 'Immunization — Human Papillomavirus',
  G844A: 'Immunization — Meningococcal C Conjugate',
  G845A: 'Immunization — Measles, Mumps, Rubella',
  G846A: 'Immunization — Pneumococcal Conjugate',
  G847A: 'Immunization — Tdap (adult)',
  G848A: 'Immunization — Varicella',

  // Spirometry (FHO only)
  J301A: 'Spirometry — Simple (VC, FEV1, FEV1/FVC, MMEFR)',
  J304A: 'Flow Volume Loop — Expiratory + Inspiratory',
  J324A: 'Spirometry — Repeat After Bronchodilator',
  J327A: 'Flow Volume Loop — Repeat After Bronchodilator',

  // Counselling/Mental Health
  K001A: 'Detention — Per Full Quarter Hour (FHO only)',
  K002A: 'Interviews with Relatives/Authorized Decision-Maker (per unit, FHO only)',
  K003A: 'Interviews with CAS/Legal Guardian (per unit, FHO only)',
  K004A: 'Psychotherapy — Family (2+ members, per unit)',
  K005A: 'Primary Mental Health Care — Individual (per unit)',
  K006A: 'Hypnotherapy — Individual (per unit)',
  K007A: 'Psychotherapy — Individual (per unit)',
  K008A: 'Diagnostic Interview/Counselling — Child/Parent (per unit)',
  K013A: 'Counselling — Individual (first 3 units K013+K040/12mo, per unit)',
  K015A: 'Counselling of Relatives — Terminally Ill Patient (per unit)',
  K017A: 'Periodic Health Visit — Child',

  // Periodic Health Visits
  K130A: 'Periodic Health Visit — Adolescent: Annual preventive health exam',
  K131A: 'Periodic Health Visit — Adult 18-64: Annual preventive health exam',
  K132A: 'Periodic Health Visit — Adult 65+: Annual preventive health exam',
  K133A: 'Periodic Health Visit — Adult with IDD: Annual preventive health exam for adults with intellectual/developmental disabilities',

  // Home Care (FHO only)
  K070A: 'Home Care Application/Supervision (FHO only)',
  K071A: 'Acute Home Care Supervision (FHO only)',

  // Case Conference/Phone Consult
  K700A: 'Palliative Care Out-Patient Case Conference (per unit)',
  K702A: 'Bariatric Out-Patient Case Conference (per unit)',
  K730A: 'Physician-to-Physician Phone Consultation — Referring',
  K731A: 'Physician-to-Physician Phone Consultation — Consultant',
  K732A: 'CritiCall Phone Consultation — Referring',
  K733A: 'CritiCall Phone Consultation — Consultant',

  // SVP — Other (FHO only)
  Q990A: 'SVP Other — Weekday Daytime (07:00-17:00)',
  Q992A: 'SVP Other — Weekday with Sacrifice of Office Hours',
  Q994A: 'SVP Other — Evening (17:00-24:00) Mon-Fri',
  Q996A: 'SVP Other — Night (00:00-07:00)',
  Q998A: 'SVP Other — Sat/Sun/Holiday (07:00-24:00)',

  // Integumentary Surgery
  R048A: 'Malignant Lesion Excision — Face/Neck, Single (FHO only)',
  R051A: 'Malignant Lesion — Laser Surgery Group 1-4 (FHO only)',
  R094A: 'Malignant Lesion Excision — Other Areas, Single (FHO only)',
  Z101A: 'Abscess/Haematoma Incision — Subcutaneous, One',
  Z110A: 'Onychogryphotic Nail — Extensive Debridement (FHO only)',
  Z113A: 'Biopsy — Any Method, Without Sutures (FHO only)',
  Z114A: 'Foreign Body Removal — Local Anaesthetic (FHO only)',
  Z116A: 'Biopsy — Any Method, With Sutures (FHO only)',
  Z117A: 'Chemical/Cryotherapy Treatment — One or More Lesions (FHO only)',
  Z122A: 'Group 3 Excision (cyst/lipoma) — Face/Neck, Single (FHO only)',
  Z125A: 'Group 3 Excision (cyst/lipoma) — Other Areas, Single (FHO only)',
  Z128A: 'Nail Plate Excision Requiring Anaesthesia — One (FHO only)',
  Z129A: 'Nail Plate Excision Requiring Anaesthesia — Multiple (FHO only)',
  Z154A: 'Laceration Repair — Up to 5cm (face/layers, FHO only)',
  Z156A: 'Group 1 Excision (keratosis) — Excision & Suture, Single (FHO only)',
  Z157A: 'Group 1 Excision (keratosis) — Excision & Suture, Two (FHO only)',
  Z158A: 'Group 1 Excision (keratosis) — Excision & Suture, Three+ (FHO only)',
  Z159A: 'Group 1 — Electrocoagulation/Curetting, Single (FHO only)',
  Z160A: 'Group 1 — Electrocoagulation/Curetting, Two (FHO only)',
  Z161A: 'Group 1 — Electrocoagulation/Curetting, Three+ (FHO only)',
  Z162A: 'Group 2 (nevus) — Excision & Suture, Single (FHO only)',
  Z175A: 'Laceration Repair — 5.1 to 10cm (FHO only)',
  Z176A: 'Laceration Repair — Up to 5cm (simple)',
  Z314A: 'Epistaxis — Cauterization, Unilateral (FHO only)',
  Z315A: 'Epistaxis — Anterior Packing, Unilateral (FHO only)',

  // GI/Urological/Eye (FHO only)
  Z535A: 'Sigmoidoscopy — Rigid Scope',
  Z543A: 'Anoscopy (Proctoscopy)',
  Z545A: 'Thrombosed Haemorrhoid(s) Incision',
  Z611A: 'Catheterization — Hospital',
  Z847A: 'Corneal Foreign Body Removal — Local Anaesthetic',

  // ═══════════════════════════════════════════════════════════════════════
  // OUT-OF-BASKET
  // ═══════════════════════════════════════════════════════════════════════

  // Consultations
  A005A: 'Consultation: Formal consultation requested by another physician — requires referral letter',
  A006A: 'Repeat Consultation: Follow-up consultation for previously consulted patient',
  A888A: 'ED Equivalent — Weekend/Holiday',
  A905A: 'Limited Consultation: Shorter consultation when full consultation not required',

  // House Calls
  A901A: 'House Call Assessment: Assessment performed at patient\'s home',
  A902A: 'House Call — Pronouncement of Death: Home visit for pronouncement of death',
  A903A: 'House Call — Additional Patient: Additional patient at same residence during house call',

  // Hospital Visits
  C001A: 'Family Practice Consultation',
  C002A: 'Repeat Consultation',
  C003A: 'Hospital Admission Assessment: Full admission assessment for hospitalized patient',
  C004A: 'Hospital Admission — Partial: Partial admission assessment',
  C009A: 'Hospital Subsequent Visit: Follow-up visit for hospitalized patient',
  C010A: 'Hospital Concurrent Care: Concurrent care visit when another physician is MRP',
  C012A: 'Hospital Discharge Day Management',
  H003A: 'Newborn Hospital Care — First Day',
  H004A: 'Newborn Hospital Care — Subsequent Day',

  // LTC
  A191A: 'LTC New Admission: Comprehensive assessment for newly admitted LTC resident',
  A192A: 'LTC Subsequent Visit: Follow-up visit for LTC resident',
  A193A: 'LTC Annual Comprehensive: Annual comprehensive assessment',
  A194A: 'LTC Intermediate Visit: Intermediate complexity visit',
  A195A: 'LTC Pronouncement of Death',

  // Prenatal/Obstetric
  P001A: 'Prenatal Visit — First: Initial prenatal visit, complete OB history + baseline',
  P002A: 'Prenatal Visit — Subsequent: Follow-up prenatal visit',
  P003A: 'Prenatal General Assessment: FIRST prenatal visit — complete OB history, baseline labs, dating',
  P004A: 'Prenatal Re-Assessment: Follow-up prenatal — fundal height, FHR, BP',
  P005A: 'Antenatal Preventive Assessment',
  P006A: 'Vaginal Delivery: Full labour and delivery',
  P007A: 'Postnatal Visit',
  P008A: 'Postnatal — Subsequent',
  P009A: 'Prenatal Late Transfer In',
  P013A: 'Labour Management — First 2 Hours',
  P014A: 'Labour Management — Each Additional Hour',
  P018A: 'Postpartum Care — Comprehensive',

  // Chronic Disease
  K022A: 'HIV Primary Care: Per unit, minimum 20 minutes',
  K023A: 'Palliative Care Support',
  K028A: 'STI Management: STI testing ordered/performed, treatment prescribed, or contact tracing',
  K029A: 'Insulin Therapy Support (max 6/year)',
  K030A: 'Diabetic Management Assessment: Active diabetes care visit — A1C review, med adjustment, foot exam (max 4/year)',
  K032A: 'Neurocognitive Assessment: FORMAL cognitive testing (MMSE, MoCA, clock drawing) — 20+ min. NOT general memory complaints or neurological exam',
  K033A: 'Counselling — Additional Units (per unit)',
  K039A: 'Smoking Cessation Follow-Up: Check-in on active quit attempt (max 2/year)',

  // Palliative
  K036A: 'Palliative Care Counselling (office): Half hour+ palliative care counselling in office',
  K037A: 'Palliative Care Counselling — Subsequent',
  K038A: 'Palliative Care — Home Visit: Palliative care counselling at patient\'s home',
  B998A: 'Home Palliative Phone Management: Per 15 min phone management for palliative patient at home',
  E082A: 'Palliative Care Premium: Add-on premium for palliative care visit',

  // Geriatric
  K655A: 'Comprehensive Geriatric Assessment: Annual comprehensive assessment for patients 75+',
  K656A: 'Geriatric Assessment — Follow-Up: Follow-up to comprehensive geriatric assessment',

  // Forms
  K031A: 'Certificate — Short: Sick note, return-to-work, brief certificate',
  K034A: 'Transfer of Care Summary: Transfer of care report or summary letter',
  K035A: 'Certificate — Long: Insurance report, disability report, detailed certificate',

  // Shared Appointments
  K140A: 'Shared Appointment (2 patients): Group chronic disease education',
  K141A: 'Shared Appointment (3 patients)',
  K142A: 'Shared Appointment (4 patients)',
  K143A: 'Shared Appointment (5 patients)',
  K144A: 'Shared Appointment (6+ patients)',

  // Premiums/Incentives
  E079A: 'Smoking Cessation — Initial Discussion (add-on)',
  E430A: 'Pap Tray Fee (with G365)',
  E431A: 'Pap Tray Fee (immunocompromised)',
  Q012A: 'After-Hours Premium: 50% premium for eligible services outside clinic hours',
  Q040A: 'Diabetes Management Incentive: After 3+ K030A visits/year — active A1C review, med adjustment',
  Q042A: 'Smoking Cessation Fee: Counselling provided — quit date, NRT, triggers discussed',
  Q050A: 'CHF Management Incentive: Active fluid status, diuretic adjustment, weight monitoring',
  Q053A: 'HCC Complex/Vulnerable Patient Bonus: $350',
  Q054A: 'Mother & Newborn Bonus: $350',
  Q010A: 'Childhood Immunization Bonus: Per completed immunization series',
  Q015A: 'Flu Immunization Bonus: Bonus for administering influenza vaccine',
  Q100A: 'Cervical Screening Bonus: Bonus for Pap/cervical screening referral',
  Q101A: 'Mammography Screening Bonus: Bonus for mammography screening referral',
  Q102A: 'Colorectal Cancer Screening Bonus: Bonus for FOBT/FIT screening referral',
  Q200A: 'Patient Rostering Fee',
  Q888A: 'Weekend Office Access Premium (FHO): Cannot bill with A888A same day',

  // eConsult
  K738A: 'eConsult — Specialist Seeking GP Input: Electronic consultation from specialist',
  K998A: 'Physician-to-Physician Phone Consultation: Phone consultation between physicians',

  // Virtual Care Modality Indicators
  K300A: 'Video Visit Modality Indicator: $0 tracking code, must accompany video visit billing',
  K301A: 'Telephone Visit Modality Indicator: $0 tracking code, must accompany phone visit billing',

  // Time-Based
  Q310: 'Direct Patient Care: In-person, video, or phone-from-office encounters. $80/hr ($20/15min)',
  Q311: 'Telephone Remote Care: Phone calls when physician is NOT in clinic. $68/hr ($17/15min)',
  Q312: 'Indirect Patient Care: Charting, lab review, referral letters, care coordination. $80/hr ($20/15min)',
  Q313: 'Clinical Administration: Screening programs, EMR updates, QI initiatives. $80/hr ($20/15min)',

  // Influenza (FHN only)
  G590A: 'Immunization — Influenza Agent (FHN only, not in FHO basket)',
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
  { name: 'Malignant excision', codes: ['R048A', 'R051A', 'R094A'], reason: 'One excision code per lesion' },
  { name: 'Laceration repair sizes', codes: ['Z154A', 'Z175A', 'Z176A'], reason: 'One repair code per wound' },
  { name: 'Group 1 excision vs electro', codes: ['Z156A', 'Z157A', 'Z158A', 'Z159A', 'Z160A', 'Z161A'], reason: 'Pick excision & suture OR electrocoagulation method — not both' },
  { name: 'Epistaxis treatment', codes: ['Z314A', 'Z315A'], reason: 'Cautery vs packing — one per encounter' },
  { name: 'Trigger point add-on', codes: ['G384A', 'G385A'], reason: 'G385 is add-on to G384 — don\'t bill both as standalone' },
  { name: 'Joint injection add-on', codes: ['G370A', 'G371A'], reason: 'G371 is add-on to G370 — requires G370 as base code' },
  { name: 'Direct care time', codes: ['Q310', 'Q311'], reason: 'In-office vs remote — one setting per encounter' },
  { name: 'FHO weekend access', codes: ['Q888A', 'A888A'], reason: 'Q888A and A888A cannot be billed same day' },
  { name: 'SVP office premiums', codes: ['A990A', 'A994A', 'A996A', 'A998A'], reason: 'One SVP office premium per visit' },
  { name: 'SVP home premiums', codes: ['B990A', 'B992A', 'B993A', 'B994A', 'B996A'], reason: 'One SVP home premium per visit' },
  { name: 'SVP other premiums', codes: ['Q990A', 'Q992A', 'Q994A', 'Q996A', 'Q998A'], reason: 'One SVP other premium per visit' },
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

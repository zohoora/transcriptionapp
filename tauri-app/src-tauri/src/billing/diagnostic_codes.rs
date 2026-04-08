//! Ontario OHIP diagnostic codes database.
//!
//! Source: Ministry of Health "Diagnostic Codes" (April 2023)
//! Updated: Bulletin 260314 (March 2026) — added 308, 489; deleted 100, 903.
//!
//! These are 3-digit codes based on ICD-8, used on OHIP claim submissions.
//! One diagnostic code per claim line. Field is 4 chars wide, left-justified.

use serde::{Deserialize, Serialize};

/// A single OHIP diagnostic code entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticCode {
    /// 3-digit code (e.g., "250")
    pub code: &'static str,
    /// Short description (e.g., "Diabetes mellitus, including complications")
    pub description: &'static str,
    /// Category grouping (e.g., "Diabetes")
    pub category: &'static str,
}

/// Total number of diagnostic codes in the database.
pub const DIAGNOSTIC_CODE_COUNT: usize = 562;

/// Static database of all OHIP diagnostic codes.
static DIAGNOSTIC_CODES: [DiagnosticCode; DIAGNOSTIC_CODE_COUNT] = [
    DiagnosticCode { code: "002", description: "Typhoid and paratyphoid fevers", category: "Intestinal Infectious Diseases" },
    DiagnosticCode { code: "003", description: "Other salmonella infections", category: "Intestinal Infectious Diseases" },
    DiagnosticCode { code: "005", description: "Food poisoning", category: "Intestinal Infectious Diseases" },
    DiagnosticCode { code: "006", description: "Amoebiasis, amoebic dysentery", category: "Intestinal Infectious Diseases" },
    DiagnosticCode { code: "009", description: "Diarrhea, gastro-enteritis, viral gastro-enteritis", category: "Intestinal Infectious Diseases" },
    DiagnosticCode { code: "010", description: "Primary tuberculous infection, including recent positive TB skin test conversion", category: "Tuberculosis" },
    DiagnosticCode { code: "011", description: "Pulmonary tuberculosis", category: "Tuberculosis" },
    DiagnosticCode { code: "012", description: "Other respiratory tuberculosis, tuberculous pleurisy with or without effusion", category: "Tuberculosis" },
    DiagnosticCode { code: "015", description: "Tuberculosis of bones and joints", category: "Tuberculosis" },
    DiagnosticCode { code: "017", description: "Tuberculosis of other organs", category: "Tuberculosis" },
    DiagnosticCode { code: "023", description: "Brucellosis", category: "Other Bacterial Diseases" },
    DiagnosticCode { code: "030", description: "Leprosy (Hansen's Disease)", category: "Other Bacterial Diseases" },
    DiagnosticCode { code: "032", description: "Diphtheria", category: "Other Bacterial Diseases" },
    DiagnosticCode { code: "033", description: "Whooping cough, pertussis", category: "Other Bacterial Diseases" },
    DiagnosticCode { code: "034", description: "Streptococcal sore throat, scarlet fever", category: "Other Bacterial Diseases" },
    DiagnosticCode { code: "035", description: "Erysipelas", category: "Other Bacterial Diseases" },
    DiagnosticCode { code: "036", description: "Meningococcal infection or meningitis", category: "Other Bacterial Diseases" },
    DiagnosticCode { code: "037", description: "Tetanus", category: "Other Bacterial Diseases" },
    DiagnosticCode { code: "038", description: "Septicemia, blood poisoning", category: "Other Bacterial Diseases" },
    DiagnosticCode { code: "039", description: "Actinomycotic infections", category: "Other Bacterial Diseases" },
    DiagnosticCode { code: "040", description: "Other bacterial diseases", category: "Other Bacterial Diseases" },
    DiagnosticCode { code: "042", description: "AIDS", category: "Human Immunodeficiency Virus (HIV) Infection" },
    DiagnosticCode { code: "043", description: "AIDS-related complex (ARC)", category: "Human Immunodeficiency Virus (HIV) Infection" },
    DiagnosticCode { code: "044", description: "Other human immunodeficiency virus infection", category: "Human Immunodeficiency Virus (HIV) Infection" },
    DiagnosticCode { code: "045", description: "Acute poliomyelitis", category: "Non-arthropod-borne Viral Diseases of Central Nervous System" },
    DiagnosticCode { code: "047", description: "Meningitis due to enterovirus", category: "Non-arthropod-borne Viral Diseases of Central Nervous System" },
    DiagnosticCode { code: "049", description: "Other non-arthropod-borne viral diseases of central nervous system", category: "Non-arthropod-borne Viral Diseases of Central Nervous System" },
    DiagnosticCode { code: "052", description: "Chickenpox", category: "Viral Diseases Accompanied by Rash" },
    DiagnosticCode { code: "053", description: "Herpes zoster, shingles", category: "Viral Diseases Accompanied by Rash" },
    DiagnosticCode { code: "054", description: "Herpes simplex, cold sore", category: "Viral Diseases Accompanied by Rash" },
    DiagnosticCode { code: "055", description: "Measles", category: "Viral Diseases Accompanied by Rash" },
    DiagnosticCode { code: "056", description: "German measles, rubella", category: "Viral Diseases Accompanied by Rash" },
    DiagnosticCode { code: "057", description: "Other viral disorders accompanied by rash (e.g., roseola)", category: "Viral Diseases Accompanied by Rash" },
    DiagnosticCode { code: "062", description: "Mosquito-borne viral encephalitis", category: "Other Viral Diseases" },
    DiagnosticCode { code: "066", description: "Other arthropod-borne viral diseases", category: "Other Viral Diseases" },
    DiagnosticCode { code: "070", description: "Viral hepatitis", category: "Other Viral Diseases" },
    DiagnosticCode { code: "072", description: "Mumps", category: "Other Viral Diseases" },
    DiagnosticCode { code: "074", description: "Diseases due to Coxsackie virus: pleurodynia, myocarditis", category: "Other Viral Diseases" },
    DiagnosticCode { code: "075", description: "Infectious mononucleosis, glandular fever", category: "Other Viral Diseases" },
    DiagnosticCode { code: "078", description: "Warts", category: "Other Viral Diseases" },
    DiagnosticCode { code: "079", description: "Other viral diseases", category: "Other Viral Diseases" },
    DiagnosticCode { code: "080", description: "Coronavirus", category: "" },
    DiagnosticCode { code: "097", description: "Syphilis-all sites and stages", category: "Venereal Diseases" },
    DiagnosticCode { code: "098", description: "Gonococcal infections", category: "Venereal Diseases" },
    DiagnosticCode { code: "099", description: "Other venereal diseases (e.g., herpes genitalis)", category: "Venereal Diseases" },
    DiagnosticCode { code: "110", description: "Ringworm of scalp, beard, or foot", category: "Mycoses" },
    DiagnosticCode { code: "112", description: "Candidiasis, monilia infection-all sites, thrush", category: "Mycoses" },
    DiagnosticCode { code: "115", description: "Histoplasmosis", category: "Mycoses" },
    DiagnosticCode { code: "117", description: "Other mycoses", category: "Mycoses" },
    DiagnosticCode { code: "122", description: "Echinococcosis, hydadid cyst-all sites", category: "Helminthiases" },
    DiagnosticCode { code: "123", description: "Taenia or tapeworm infestation-all types", category: "Helminthiases" },
    DiagnosticCode { code: "127", description: "Pinworm infestation", category: "Helminthiases" },
    DiagnosticCode { code: "128", description: "Other helminthiases", category: "Helminthiases" },
    DiagnosticCode { code: "130", description: "Toxoplasmosis", category: "Other Infectious and Parasitic Diseases" },
    DiagnosticCode { code: "131", description: "Trichomonas infection", category: "Other Infectious and Parasitic Diseases" },
    DiagnosticCode { code: "132", description: "Head or body lice, pediculosis", category: "Other Infectious and Parasitic Diseases" },
    DiagnosticCode { code: "133", description: "Scabies, acariasis", category: "Other Infectious and Parasitic Diseases" },
    DiagnosticCode { code: "135", description: "Sarcoidosis", category: "Other Infectious and Parasitic Diseases" },
    DiagnosticCode { code: "136", description: "Other infectious or parasitic diseases", category: "Other Infectious and Parasitic Diseases" },
    DiagnosticCode { code: "140", description: "Lip", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "141", description: "Tongue", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "142", description: "Major salivary glands", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "143", description: "Gum", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "144", description: "Floor of mouth", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "145", description: "Other and unspecified parts of mouth", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "146", description: "Oropharynx", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "147", description: "Nasopharynx", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "148", description: "Hypopharynx", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "149", description: "Other and ill-defined sites within the lip, oral cavity, and pharynx", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "150", description: "Esophagus", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "151", description: "Stomach", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "152", description: "Small intestine, including duodenum", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "153", description: "Large intestine-excluding rectum", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "154", description: "Rectum, rectosigmoid and anus", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "155", description: "Primary malignancy of liver (not secondary spread or metastatic disease)", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "156", description: "Gallbladder and extra hepatic bile ducts", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "157", description: "Pancreas", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "158", description: "Retroperitoneum and peritoneum", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "159", description: "Other and ill-defined sites within the digestive organs and peritoneum", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "160", description: "Nasal cavities, middle ear, and accessory sinuses", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "161", description: "Larynx, trachea", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "162", description: "Bronchus, lung", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "163", description: "Pleura", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "164", description: "Thymus, heart, and mediastinum", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "165", description: "Other sites within the respiratory system and intrathoracic organs", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "170", description: "Bone", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "171", description: "Connective and other soft tissue", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "172", description: "Melanoma of skin", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "173", description: "Other skin malignancies", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "174", description: "Female breast", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "175", description: "Male breast", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "179", description: "Uterus, part unspecified", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "180", description: "Cervix", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "181", description: "Placenta", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "182", description: "Body of uterus", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "183", description: "Ovary, fallopian tube, broad ligament", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "184", description: "Vagina, vulva, other female genital organs", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "185", description: "Prostate", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "186", description: "Testis", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "187", description: "Other male genital organs", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "188", description: "Bladder", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "189", description: "Kidney, other urinary organs", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "190", description: "Eye", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "191", description: "Brain", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "192", description: "Cranial nerves, spinal cord, other parts of nervous system", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "193", description: "Thyroid", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "194", description: "Other endocrine glands and related structures", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "195", description: "Other ill-defined sites", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "196", description: "Secondary neoplasm of lymph nodes", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "197", description: "Secondary neoplasm of respiratory and digestive systems", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "198", description: "Metastatic or secondary malignant neoplasm, carcinomatosis", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "199", description: "Other malignant neoplasms", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "200", description: "Lymphosarcoma, reticulosarcoma", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "201", description: "Hodgkin's disease", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "202", description: "Other malignant neoplasms of lymphoid and histiocytic tissue", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "203", description: "Multiple myeloma, plasma cell leukemia", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "204", description: "Lymphoid leukemia (including lymphatic and histiocytic leukemia)", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "205", description: "Myeloid leukemia (including granulocytic and myelogenous leukemia)", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "206", description: "Monocytic leukemia", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "207", description: "Other specified leukemia", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "208", description: "Other types of leukemia", category: "Malignant Neoplasms" },
    DiagnosticCode { code: "210", description: "Lip, oral cavity, pharynx", category: "Benign Neoplasms" },
    DiagnosticCode { code: "211", description: "Other parts of digestive system, peritoneum", category: "Benign Neoplasms" },
    DiagnosticCode { code: "212", description: "Respiratory and intra-thoracic organs", category: "Benign Neoplasms" },
    DiagnosticCode { code: "213", description: "Bone, cartilage", category: "Benign Neoplasms" },
    DiagnosticCode { code: "214", description: "Lipoma", category: "Benign Neoplasms" },
    DiagnosticCode { code: "215", description: "Connective and other soft tissue", category: "Benign Neoplasms" },
    DiagnosticCode { code: "216", description: "Skin (e.g., pigmented naevus, dermatofibroma)", category: "Benign Neoplasms" },
    DiagnosticCode { code: "217", description: "Breast", category: "Benign Neoplasms" },
    DiagnosticCode { code: "218", description: "Uterine fibroid, leiomyoma", category: "Benign Neoplasms" },
    DiagnosticCode { code: "219", description: "Other benign neoplasms of uterus (e.g., cervical polyp)", category: "Benign Neoplasms" },
    DiagnosticCode { code: "220", description: "Ovary (e.g., ovarian cyst)", category: "Benign Neoplasms" },
    DiagnosticCode { code: "221", description: "Other benign neoplasms of female genital organs", category: "Benign Neoplasms" },
    DiagnosticCode { code: "222", description: "Benign neoplasms of male genital organs", category: "Benign Neoplasms" },
    DiagnosticCode { code: "223", description: "Kidney, ureter, bladder", category: "Benign Neoplasms" },
    DiagnosticCode { code: "224", description: "Eye", category: "Benign Neoplasms" },
    DiagnosticCode { code: "225", description: "Brain, spinal cord, peripheral nerves", category: "Benign Neoplasms" },
    DiagnosticCode { code: "226", description: "Thyroid (e.g., adenoma or cystadenoma)", category: "Benign Neoplasms" },
    DiagnosticCode { code: "227", description: "Other endocrine glands and related structures", category: "Benign Neoplasms" },
    DiagnosticCode { code: "228", description: "Haemangioma and lymphangiomax", category: "Benign Neoplasms" },
    DiagnosticCode { code: "229", description: "Other benign neoplasms", category: "Benign Neoplasms" },
    DiagnosticCode { code: "230", description: "Digestive organs", category: "Carcinoma in Situ" },
    DiagnosticCode { code: "231", description: "Respiratory system", category: "Carcinoma in Situ" },
    DiagnosticCode { code: "232", description: "Skin", category: "Carcinoma in Situ" },
    DiagnosticCode { code: "233", description: "Breast and genito-urinary system", category: "Carcinoma in Situ" },
    DiagnosticCode { code: "234", description: "Other", category: "Carcinoma in Situ" },
    DiagnosticCode { code: "235", description: "Digestive and respiratory systems", category: "Neoplasms of Uncertain Behavior" },
    DiagnosticCode { code: "236", description: "Genitourinary organs", category: "Neoplasms of Uncertain Behavior" },
    DiagnosticCode { code: "237", description: "Endocrine glands and nervous system", category: "Neoplasms of Uncertain Behavior" },
    DiagnosticCode { code: "238", description: "Other and unspecified sites and tissues", category: "Neoplasms of Uncertain Behavior" },
    DiagnosticCode { code: "239", description: "Unspecified neoplasms (e.g., polycythemia vera)", category: "Neoplasms of Uncertain Behavior" },
    DiagnosticCode { code: "240", description: "Simple thyroid goitre", category: "Endocrine Glands" },
    DiagnosticCode { code: "241", description: "Nontoxic nodular goitre", category: "Endocrine Glands" },
    DiagnosticCode { code: "242", description: "Hyperthyroidism, thyrotoxicosis, exophthalmic goitre", category: "Endocrine Glands" },
    DiagnosticCode { code: "243", description: "Hypothyroidism - congenital (i.e., cretinism)", category: "Endocrine Glands" },
    DiagnosticCode { code: "244", description: "Hypothyroidism - acquired (i.e., myxedema)", category: "Endocrine Glands" },
    DiagnosticCode { code: "245", description: "Thyroiditis", category: "Endocrine Glands" },
    DiagnosticCode { code: "248", description: "mellitus with ocular complications", category: "Diabetes" },
    DiagnosticCode { code: "249", description: "Pre-diabetes", category: "Diabetes" },
    DiagnosticCode { code: "250", description: "Diabetes mellitus, including complications", category: "Diabetes" },
    DiagnosticCode { code: "251", description: "Other disorders of pancreatic internal secretions (e.g., insulinoma neo-natal hypoglycemia, Zollinger -Ellison syndrome)", category: "Diabetes" },
    DiagnosticCode { code: "252", description: "Parathyroid gland disorders (e.g., hyperparathyroidism, hypoparathyroidism)", category: "Diabetes" },
    DiagnosticCode { code: "253", description: "Pituitary gland disorders (e.g., acromegaly, dwarfism, diabetes insipidus)", category: "Diabetes" },
    DiagnosticCode { code: "255", description: "Adrenal gland disorders (e.g., Cushing's syndrome, hyperaldosteronism, Conn's syndrome, adrenogenital syndrome, Addison's disease)", category: "Diabetes" },
    DiagnosticCode { code: "256", description: "Ovarian dysfunction (e.g., ovarian failure, polycystic ovaries, Stein-Leventhal syndrome)", category: "Diabetes" },
    DiagnosticCode { code: "257", description: "Testicular dysfunction", category: "Diabetes" },
    DiagnosticCode { code: "259", description: "Other endocrine disorders", category: "Diabetes" },
    DiagnosticCode { code: "263", description: "Unspecified malnutrition", category: "Nutritional and Metabolic Disorders" },
    DiagnosticCode { code: "269", description: "Vitamin and other nutritional deficiencies", category: "Nutritional and Metabolic Disorders" },
    DiagnosticCode { code: "270", description: "Disorders of amino-acid metabolism (e.g., cystinuria, Fanconi syndrome)", category: "Nutritional and Metabolic Disorders" },
    DiagnosticCode { code: "272", description: "Disorders of lipoid metabolism (e.g., hypercholesterolemia, lipoprotein disorders)", category: "Nutritional and Metabolic Disorders" },
    DiagnosticCode { code: "274", description: "Gout", category: "Nutritional and Metabolic Disorders" },
    DiagnosticCode { code: "277", description: "Other metabolic disorders", category: "Nutritional and Metabolic Disorders" },
    DiagnosticCode { code: "278", description: "Obesity", category: "Nutritional and Metabolic Disorders" },
    DiagnosticCode { code: "279", description: "Hypogammaglobulinemia, agammaglobulinemia, other immunity disorders", category: "Immunity Disorders" },
    DiagnosticCode { code: "280", description: "Iron deficiency anaemia", category: "" },
    DiagnosticCode { code: "281", description: "Pernicious anaemia", category: "" },
    DiagnosticCode { code: "282", description: "Hereditary hemolytic anaemia (e.g., thalassemia, sickle-cell anaemia)", category: "" },
    DiagnosticCode { code: "283", description: "Acquired hemolytic anaemia, excluding hemolytic disease of newborn", category: "" },
    DiagnosticCode { code: "284", description: "Aplastic anaemia", category: "" },
    DiagnosticCode { code: "285", description: "Other anaemias", category: "" },
    DiagnosticCode { code: "286", description: "Coagulation defects (e.g., hemophilia, other factor deficiencies)", category: "" },
    DiagnosticCode { code: "287", description: "Purpura, thrombocytopenia, other hemorrhagic conditions", category: "" },
    DiagnosticCode { code: "288", description: "Neutropenia, acranulocytosis, eosinophilia", category: "" },
    DiagnosticCode { code: "289", description: "Other diseases of blood, marrow, spleen", category: "" },
    DiagnosticCode { code: "290", description: "Senile dementia, presenile dementia", category: "Psychoses" },
    DiagnosticCode { code: "291", description: "Alcoholic psychosis, delirium tremens, Korsakov's psychosis", category: "Psychoses" },
    DiagnosticCode { code: "292", description: "Drug psychosis", category: "Psychoses" },
    DiagnosticCode { code: "295", description: "Schizophrenia", category: "Psychoses" },
    DiagnosticCode { code: "296", description: "Manic depressive psychosis, involutional melancholia", category: "Psychoses" },
    DiagnosticCode { code: "297", description: "Paranoid states", category: "Psychoses" },
    DiagnosticCode { code: "298", description: "Other psychoses", category: "Psychoses" },
    DiagnosticCode { code: "299", description: "Childhood psychoses (e.g., autism)", category: "Psychoses" },
    DiagnosticCode { code: "300", description: "Anxiety neurosis, hysteria, neurasthenia, obsessive compulsive neurosis, reactive depression", category: "Neuroses and Personality Disorders" },
    DiagnosticCode { code: "301", description: "Personality disorders (e.g., paranoid personality, schizoid personality, obsessive compulsive personality)", category: "Neuroses and Personality Disorders" },
    DiagnosticCode { code: "302", description: "Sexual deviations", category: "Neuroses and Personality Disorders" },
    DiagnosticCode { code: "303", description: "Alcoholism", category: "Neuroses and Personality Disorders" },
    DiagnosticCode { code: "304", description: "Drug dependence, drug addiction", category: "Neuroses and Personality Disorders" },
    DiagnosticCode { code: "305", description: "Tobacco abuse", category: "Neuroses and Personality Disorders" },
    DiagnosticCode { code: "306", description: "Psychosomatic disturbances", category: "Neuroses and Personality Disorders" },
    DiagnosticCode { code: "307", description: "Habit spasms, tics, stuttering, tension headaches, anorexia nervosa, sleep disorders, enuresis", category: "Neuroses and Personality Disorders" },
    DiagnosticCode { code: "308", description: "Gender Dysphoria", category: "Mental Disorders" },
    DiagnosticCode { code: "309", description: "Adjustment reaction", category: "Neuroses and Personality Disorders" },
    DiagnosticCode { code: "311", description: "Depressive or other non-psychotic disorders, not elsewhere classified", category: "Neuroses and Personality Disorders" },
    DiagnosticCode { code: "313", description: "Behaviour disorders of childhood and adolescence", category: "Neuroses and Personality Disorders" },
    DiagnosticCode { code: "314", description: "Hyperkinetic syndrome of childhood", category: "Neuroses and Personality Disorders" },
    DiagnosticCode { code: "315", description: "Specified delays in development (e.g., dyslexia, dyslalia, motor retardation)", category: "Neuroses and Personality Disorders" },
    DiagnosticCode { code: "319", description: "Mental retardation", category: "Neuroses and Personality Disorders" },
    DiagnosticCode { code: "320", description: "Bacterial meningitis", category: "Central Nervous System" },
    DiagnosticCode { code: "321", description: "Meningitis due to other organisms", category: "Central Nervous System" },
    DiagnosticCode { code: "323", description: "Encephalitis, encephalomyelitis", category: "Central Nervous System" },
    DiagnosticCode { code: "330", description: "Tay-Sachs disease", category: "Central Nervous System" },
    DiagnosticCode { code: "331", description: "Other cerebral degenerations", category: "Central Nervous System" },
    DiagnosticCode { code: "332", description: "Parkinson's disease", category: "Central Nervous System" },
    DiagnosticCode { code: "335", description: "Anterior Horn Cell Disease", category: "Central Nervous System" },
    DiagnosticCode { code: "340", description: "Multiple sclerosis", category: "Central Nervous System" },
    DiagnosticCode { code: "343", description: "Cerebral palsy", category: "Central Nervous System" },
    DiagnosticCode { code: "345", description: "Epilepsy", category: "Central Nervous System" },
    DiagnosticCode { code: "346", description: "Migraine", category: "Central Nervous System" },
    DiagnosticCode { code: "349", description: "Other diseases of central nervous system (e.g., brain abscess, narcolepsy, motor neuron disease, syringomyelia)", category: "Central Nervous System" },
    DiagnosticCode { code: "350", description: "Trigeminal neuralgia, tic douloureux", category: "Peripheral Nervous System" },
    DiagnosticCode { code: "351", description: "Bell's palsy, facial nerve disorders", category: "Peripheral Nervous System" },
    DiagnosticCode { code: "352", description: "Disorders of other cranial nerves", category: "Peripheral Nervous System" },
    DiagnosticCode { code: "356", description: "Idiopathic peripheral neuritis", category: "Peripheral Nervous System" },
    DiagnosticCode { code: "358", description: "Myoneural disorders (e.g., myasthenia gravis)", category: "Peripheral Nervous System" },
    DiagnosticCode { code: "359", description: "Muscular dystrophies", category: "Peripheral Nervous System" },
    DiagnosticCode { code: "360", description: "Aphakia", category: "Eye" },
    DiagnosticCode { code: "361", description: "Retinal detachment", category: "Eye" },
    DiagnosticCode { code: "362", description: "Hypertensive retinopathy and other retinal diseases not specifically listed", category: "Eye" },
    DiagnosticCode { code: "363", description: "Chorioretinitis", category: "Eye" },
    DiagnosticCode { code: "364", description: "Iritis", category: "Eye" },
    DiagnosticCode { code: "365", description: "Glaucoma", category: "Eye" },
    DiagnosticCode { code: "366", description: "Cataract, excludes diabetic or congenital", category: "Eye" },
    DiagnosticCode { code: "367", description: "Myopia, astigmatism (except for the specific conditions defined by diagnostic code 371), presbyopia and other disorders of refraction and accommodation", category: "Eye" },
    DiagnosticCode { code: "368", description: "Amblyopia, visual field defects", category: "Eye" },
    DiagnosticCode { code: "369", description: "Blindness and low vision", category: "Eye" },
    DiagnosticCode { code: "370", description: "Keratitis, corneal ulcer", category: "Eye" },
    DiagnosticCode { code: "371", description: "High Myopia greater than 9 diopters; Irregular Astigmatism resulting from corneal grafting or corneal scarring from diseases", category: "Eye" },
    DiagnosticCode { code: "372", description: "Conjunctiva disorders (e.g., conjunctivitis, pterygium)", category: "Eye" },
    DiagnosticCode { code: "373", description: "Blepharitis, chalazion, stye", category: "Eye" },
    DiagnosticCode { code: "374", description: "Other eyelid disorders (e.g., entropion, ectropion, ptosis)", category: "Eye" },
    DiagnosticCode { code: "375", description: "Dacryocystitis, obstruction of lacrimal duct", category: "Eye" },
    DiagnosticCode { code: "376", description: "Keratoconus", category: "Eye" },
    DiagnosticCode { code: "377", description: "Optic neuritis", category: "Eye" },
    DiagnosticCode { code: "378", description: "Strabismus", category: "Eye" },
    DiagnosticCode { code: "379", description: "Other disorders of the eye", category: "Eye" },
    DiagnosticCode { code: "380", description: "Otitis externa", category: "Ear and Mastoid" },
    DiagnosticCode { code: "381", description: "Serous otitis media, eustachian tube disorders", category: "Ear and Mastoid" },
    DiagnosticCode { code: "382", description: "Suppurative otitis media", category: "Ear and Mastoid" },
    DiagnosticCode { code: "383", description: "Mastoiditis", category: "Ear and Mastoid" },
    DiagnosticCode { code: "384", description: "Perforation of tympanic membrane", category: "Ear and Mastoid" },
    DiagnosticCode { code: "386", description: "Meniere's disease, labyrinthitis", category: "Ear and Mastoid" },
    DiagnosticCode { code: "387", description: "Otosclerosis", category: "Ear and Mastoid" },
    DiagnosticCode { code: "388", description: "Wax or cerumen in ear, other disorders of ear and mastoid, tinnitus", category: "Ear and Mastoid" },
    DiagnosticCode { code: "389", description: "Deafness", category: "Ear and Mastoid" },
    DiagnosticCode { code: "390", description: "Rheumatic fever without endocarditis, myocarditis or pericarditis", category: "Rheumatic Fever and Rheumatic Heart Disease" },
    DiagnosticCode { code: "391", description: "Rheumatic fever with endocarditis, myocarditis, or pericarditis", category: "Rheumatic Fever and Rheumatic Heart Disease" },
    DiagnosticCode { code: "392", description: "Chorea", category: "Rheumatic Fever and Rheumatic Heart Disease" },
    DiagnosticCode { code: "394", description: "Mitral stenosis, mitral insufficiency", category: "Rheumatic Fever and Rheumatic Heart Disease" },
    DiagnosticCode { code: "398", description: "Other rheumatic heart disease", category: "Rheumatic Fever and Rheumatic Heart Disease" },
    DiagnosticCode { code: "401", description: "Essential, benign hypertension", category: "Hypertensive Disease" },
    DiagnosticCode { code: "402", description: "Hypertensive heart disease", category: "Hypertensive Disease" },
    DiagnosticCode { code: "403", description: "Hypertensive renal disease", category: "Hypertensive Disease" },
    DiagnosticCode { code: "410", description: "Acute myocardial infarction", category: "Ischaemic and Other Forms of Heart Disease" },
    DiagnosticCode { code: "412", description: "Old myocardial infarction, chronic coronary artery disease of arteriosclerotic heart disease, without symptoms", category: "Ischaemic and Other Forms of Heart Disease" },
    DiagnosticCode { code: "413", description: "Acute coronary insufficiency, angina pectoris, acute ischaemic heart disease", category: "Ischaemic and Other Forms of Heart Disease" },
    DiagnosticCode { code: "415", description: "Pulmonary embolism, pulmonary infarction", category: "Ischaemic and Other Forms of Heart Disease" },
    DiagnosticCode { code: "426", description: "Heart blocks, other conduction disorders", category: "Ischaemic and Other Forms of Heart Disease" },
    DiagnosticCode { code: "427", description: "Paroxysmal tachycardia, atrial or ventricular flutter or fibrillation, cardiac arrest, other arrythmias", category: "Ischaemic and Other Forms of Heart Disease" },
    DiagnosticCode { code: "428", description: "Congestive heart failure", category: "Ischaemic and Other Forms of Heart Disease" },
    DiagnosticCode { code: "429", description: "All other forms of heart disease", category: "Ischaemic and Other Forms of Heart Disease" },
    DiagnosticCode { code: "432", description: "Intracranial Haemorrhage", category: "Cerebrovascular Disease" },
    DiagnosticCode { code: "435", description: "Transient cerebral ischaemia", category: "Cerebrovascular Disease" },
    DiagnosticCode { code: "436", description: "Acute cerebrovascular accident, C.V.A., stroke", category: "Cerebrovascular Disease" },
    DiagnosticCode { code: "437", description: "Chronic arteriosclerotic cerebrovascular disease, hypertensive encephalopathy", category: "Cerebrovascular Disease" },
    DiagnosticCode { code: "440", description: "Generalized arteriosclerosis, atherosclerosis", category: "Diseases of Arteries" },
    DiagnosticCode { code: "441", description: "Aortic aneurysm (non-syphilitic)", category: "Diseases of Arteries" },
    DiagnosticCode { code: "443", description: "Raynaud's disease, Buerger's disease, peripheral vascular disease, intermittent claudication", category: "Diseases of Arteries" },
    DiagnosticCode { code: "446", description: "Polyarteritis nodosa, temporal arteritis", category: "Diseases of Arteries" },
    DiagnosticCode { code: "447", description: "Other disorders of arteries", category: "Diseases of Arteries" },
    DiagnosticCode { code: "451", description: "Phlebitis, thrombophlebitis", category: "Diseases of Veins and Lyphatics" },
    DiagnosticCode { code: "452", description: "Portal vein thrombosis", category: "Diseases of Veins and Lyphatics" },
    DiagnosticCode { code: "454", description: "Varicose veins of lower extremities with or without ulcer", category: "Diseases of Veins and Lyphatics" },
    DiagnosticCode { code: "455", description: "Haemorrhoids", category: "Diseases of Veins and Lyphatics" },
    DiagnosticCode { code: "457", description: "Lymphangitis, lymphedema", category: "Diseases of Veins and Lyphatics" },
    DiagnosticCode { code: "459", description: "Other disorders of circulatory system", category: "Diseases of Veins and Lyphatics" },
    DiagnosticCode { code: "460", description: "Acute nasopharyngitis, common cold", category: "" },
    DiagnosticCode { code: "461", description: "Acute sinusitis", category: "" },
    DiagnosticCode { code: "463", description: "Acute tonsillitis", category: "" },
    DiagnosticCode { code: "464", description: "Acute laryngitis, tracheitis, croup, epiglottis", category: "" },
    DiagnosticCode { code: "466", description: "Acute bronchitis", category: "" },
    DiagnosticCode { code: "470", description: "Deviated nasal septum", category: "" },
    DiagnosticCode { code: "471", description: "Nasal polyp", category: "" },
    DiagnosticCode { code: "473", description: "Chronic sinusitis", category: "" },
    DiagnosticCode { code: "474", description: "Hypertrophy or chronic infection of tonsils and/or adenoids", category: "" },
    DiagnosticCode { code: "477", description: "Allergic rhinitis, hay fever", category: "" },
    DiagnosticCode { code: "486", description: "Pneumonia - all types", category: "" },
    DiagnosticCode { code: "487", description: "Influenza", category: "" },
    DiagnosticCode { code: "489", description: "Respiratory syncytial virus (RSV)", category: "Diseases of the Respiratory System" },
    DiagnosticCode { code: "491", description: "Chronic bronchitis", category: "" },
    DiagnosticCode { code: "492", description: "Emphysema", category: "" },
    DiagnosticCode { code: "493", description: "Asthma, allergic bronchitis", category: "" },
    DiagnosticCode { code: "494", description: "Bronchiectasis", category: "" },
    DiagnosticCode { code: "496", description: "Other chronic obstructive pulmonary disease", category: "" },
    DiagnosticCode { code: "501", description: "Asbestosis", category: "" },
    DiagnosticCode { code: "502", description: "Silicosis", category: "" },
    DiagnosticCode { code: "511", description: "Pleurisy with or without effusion", category: "" },
    DiagnosticCode { code: "512", description: "Spontaneous pneumothorax, tension pneumothorax", category: "" },
    DiagnosticCode { code: "515", description: "Pulmonary fibrosis", category: "" },
    DiagnosticCode { code: "518", description: "Atelectasis, other diseases of lung", category: "" },
    DiagnosticCode { code: "519", description: "Other diseases of respiratory system", category: "" },
    DiagnosticCode { code: "521", description: "Dental caries, other diseases of hard tissues of teeth (system inserted for dentists' claims)", category: "Diseases of Oral Cavity, Salivary Glands and Jaws" },
    DiagnosticCode { code: "523", description: "Gingivitis, periodontal disease", category: "Diseases of Oral Cavity, Salivary Glands and Jaws" },
    DiagnosticCode { code: "524", description: "Prognathism, micrognathism, macrognathism, retrognathism, malocclusion, temporomandibular joint disorders", category: "Diseases of Oral Cavity, Salivary Glands and Jaws" },
    DiagnosticCode { code: "525", description: "Other conditions of teeth and supporting structure", category: "Diseases of Oral Cavity, Salivary Glands and Jaws" },
    DiagnosticCode { code: "527", description: "Disease of salivary glands", category: "Diseases of Oral Cavity, Salivary Glands and Jaws" },
    DiagnosticCode { code: "528", description: "Stomatitis, aphthous ulcers, canker sore, diseases of lips", category: "Diseases of Oral Cavity, Salivary Glands and Jaws" },
    DiagnosticCode { code: "529", description: "Glossitis, other conditions of the tongue", category: "Diseases of Oral Cavity, Salivary Glands and Jaws" },
    DiagnosticCode { code: "530", description: "Esophagitis, cardiospasm, ulcer of esophagus; stricture, stenosis, or obstruction of esophagus", category: "Diseases of Esophagus, Stomach and Duodenum" },
    DiagnosticCode { code: "531", description: "Gastric ulcer, with or without haemorrage or perforation", category: "Diseases of Esophagus, Stomach and Duodenum" },
    DiagnosticCode { code: "532", description: "Duodenal ulcer, with or without haemorrhage or perforation", category: "Diseases of Esophagus, Stomach and Duodenum" },
    DiagnosticCode { code: "534", description: "Stomal ulcer, gastrojejunal ulcer", category: "Diseases of Esophagus, Stomach and Duodenum" },
    DiagnosticCode { code: "535", description: "Gastritis", category: "Diseases of Esophagus, Stomach and Duodenum" },
    DiagnosticCode { code: "536", description: "Hyperchlorhydria, hypochlorhydria, dyspepsia, indigestion", category: "Diseases of Esophagus, Stomach and Duodenum" },
    DiagnosticCode { code: "537", description: "Other disorders of stomach and duodenum", category: "Diseases of Esophagus, Stomach and Duodenum" },
    DiagnosticCode { code: "540", description: "Acute appendicitis, with or without abscess or peritonitis", category: "Other Diseases of Intestine and Peritoneum" },
    DiagnosticCode { code: "545", description: "Colon Positive Fecal Occult Blood", category: "Diseases of Esophagus, Stomach and Duodenum" },
    DiagnosticCode { code: "546", description: "Colon Surveillance", category: "Diseases of Esophagus, Stomach and Duodenum" },
    DiagnosticCode { code: "547", description: "Colon Family history of colon cancer", category: "Diseases of Esophagus, Stomach and Duodenum" },
    DiagnosticCode { code: "548", description: "Colon Screening", category: "Diseases of Esophagus, Stomach and Duodenum" },
    DiagnosticCode { code: "550", description: "Inguinal hernia, with or without obstruction", category: "Hernia" },
    DiagnosticCode { code: "552", description: "Femoral, umbilical, ventral, diaphragmatic or hiatus hernia with obstruction", category: "Hernia" },
    DiagnosticCode { code: "553", description: "Femoral, umbilical, ventral, diaphragmatic or hiatus hernia without obstruction", category: "Hernia" },
    DiagnosticCode { code: "555", description: "Regional enteritis, Crohn's disease", category: "Other Diseases of Intestine and Peritoneum" },
    DiagnosticCode { code: "556", description: "Ulcerative colitis", category: "Other Diseases of Intestine and Peritoneum" },
    DiagnosticCode { code: "557", description: "Mesenteric artery occlusion, other vascular conditions of intestine", category: "Other Diseases of Intestine and Peritoneum" },
    DiagnosticCode { code: "560", description: "Intestinal obstruction, intussusception, paralytic ileus, volvulus, impaction of intestine", category: "Other Diseases of Intestine and Peritoneum" },
    DiagnosticCode { code: "562", description: "Diverticulitis or diverticulosis of large or small intestine", category: "Other Diseases of Intestine and Peritoneum" },
    DiagnosticCode { code: "564", description: "Spastic colon, irritable colon, mucous colitis, constipation", category: "Other Diseases of Intestine and Peritoneum" },
    DiagnosticCode { code: "565", description: "Anal fissure, anal fistula", category: "Other Diseases of Intestine and Peritoneum" },
    DiagnosticCode { code: "566", description: "Abscess of anal or rectal regions", category: "Other Diseases of Intestine and Peritoneum" },
    DiagnosticCode { code: "567", description: "Peritonitis, with or without abscess", category: "Other Diseases of Intestine and Peritoneum" },
    DiagnosticCode { code: "569", description: "Anal or rectal polyp, rectal prolapse, anal or rectal stricture, rectal bleeding, other disorders of intestine", category: "Other Diseases of Intestine and Peritoneum" },
    DiagnosticCode { code: "571", description: "Cirrhosis of the liver (e.g., alcoholic cirrhosis, biliary cirrhosis)", category: "Other Diseases of Digestive System" },
    DiagnosticCode { code: "573", description: "Other diseases of the liver", category: "Other Diseases of Digestive System" },
    DiagnosticCode { code: "574", description: "Cholelithiasis (gall stones) with or without cholecystitis", category: "Other Diseases of Digestive System" },
    DiagnosticCode { code: "575", description: "Cholecystitis, without gall stones", category: "Other Diseases of Digestive System" },
    DiagnosticCode { code: "576", description: "Other diseases of gallbladder and biliary ducts", category: "Other Diseases of Digestive System" },
    DiagnosticCode { code: "577", description: "Diseases of pancreas", category: "Other Diseases of Digestive System" },
    DiagnosticCode { code: "579", description: "Malabsorption syndrome, sprue, celiac disease", category: "Other Diseases of Digestive System" },
    DiagnosticCode { code: "580", description: "Acute glomerulonephritis", category: "Diseases of the Urinary System" },
    DiagnosticCode { code: "581", description: "Nephrotic Syndrome", category: "Diseases of the Urinary System" },
    DiagnosticCode { code: "584", description: "Acute renal failure", category: "Diseases of the Urinary System" },
    DiagnosticCode { code: "585", description: "Chronic renal failure, uremia", category: "Diseases of the Urinary System" },
    DiagnosticCode { code: "590", description: "Acute or chronic pyelonephritis, pyelitis, abscess", category: "Diseases of the Urinary System" },
    DiagnosticCode { code: "591", description: "Hydronephrosis", category: "Diseases of the Urinary System" },
    DiagnosticCode { code: "592", description: "Stone in kidney or ureter", category: "Diseases of the Urinary System" },
    DiagnosticCode { code: "593", description: "Other disorders of kidney or ureter", category: "Diseases of the Urinary System" },
    DiagnosticCode { code: "595", description: "Cystitis", category: "Diseases of the Urinary System" },
    DiagnosticCode { code: "597", description: "Non-specific urethritis (not sexually transmitted)", category: "Diseases of the Urinary System" },
    DiagnosticCode { code: "598", description: "Urethral stricture", category: "Diseases of the Urinary System" },
    DiagnosticCode { code: "599", description: "Other disorders of urinary tract", category: "Diseases of the Urinary System" },
    DiagnosticCode { code: "600", description: "Benign prostatic hypertrophy", category: "Diseases of Male Genital Organs" },
    DiagnosticCode { code: "601", description: "Prostatitis", category: "Diseases of Male Genital Organs" },
    DiagnosticCode { code: "603", description: "Hydrocele", category: "Diseases of Male Genital Organs" },
    DiagnosticCode { code: "604", description: "Orchitis, epididymitis", category: "Diseases of Male Genital Organs" },
    DiagnosticCode { code: "605", description: "Phimosis, paraphimosis", category: "Diseases of Male Genital Organs" },
    DiagnosticCode { code: "606", description: "Male infertility, oligospermia, azoospermia", category: "Diseases of Male Genital Organs" },
    DiagnosticCode { code: "608", description: "Seminal vesiculitis, spermatocele, torsion of cord or testis, undescended testicle, other disorders of male genital organs", category: "Diseases of Male Genital Organs" },
    DiagnosticCode { code: "609", description: "Newborn circumcision", category: "Diseases of Male Genital Organs" },
    DiagnosticCode { code: "610", description: "Cystic mastitis, chronic cystic disease, breast cyst, fibro-adenosis of breast", category: "Diseases of Breast and Female Pelvic Organs" },
    DiagnosticCode { code: "611", description: "Breast abscess, gynecomastia, hypertrophy, other disorders of breast", category: "Diseases of Breast and Female Pelvic Organs" },
    DiagnosticCode { code: "614", description: "Acute or chronic salpingitis or oophoritis or abscess, pelvic inflammatory disease", category: "Diseases of Breast and Female Pelvic Organs" },
    DiagnosticCode { code: "615", description: "Acute or chronic endometritis", category: "Diseases of Breast and Female Pelvic Organs" },
    DiagnosticCode { code: "616", description: "Cervicitis, vaginitis, cyst or abscess of Bartholin's gland, vulvitis", category: "Diseases of Breast and Female Pelvic Organs" },
    DiagnosticCode { code: "617", description: "Endometriosis", category: "Other Disorders of Female Genital Tract" },
    DiagnosticCode { code: "618", description: "Cystocele, rectocele, urethrocele, enterocele, uterine prolapse", category: "Other Disorders of Female Genital Tract" },
    DiagnosticCode { code: "621", description: "Retroversion of uterus, endometrial hyperplasia, other disorders of uteru", category: "Other Disorders of Female Genital Tract" },
    DiagnosticCode { code: "622", description: "Cervical erosion, cervical dysplasia", category: "Other Disorders of Female Genital Tract" },
    DiagnosticCode { code: "623", description: "Stricture or stenosis of vagina", category: "Other Disorders of Female Genital Tract" },
    DiagnosticCode { code: "625", description: "Dyspareunia, dysmenorrhea, premenstrual tension, stress incontinence", category: "Other Disorders of Female Genital Tract" },
    DiagnosticCode { code: "626", description: "Disorders of menstruation", category: "Other Disorders of Female Genital Tract" },
    DiagnosticCode { code: "627", description: "Menopause, post-menopausal bleeding", category: "Other Disorders of Female Genital Tract" },
    DiagnosticCode { code: "628", description: "Infertility", category: "Other Disorders of Female Genital Tract" },
    DiagnosticCode { code: "629", description: "Other disorders of female genital organs", category: "Other Disorders of Female Genital Tract" },
    DiagnosticCode { code: "632", description: "Missed abortion", category: "" },
    DiagnosticCode { code: "633", description: "Ectopic pregnancy", category: "" },
    DiagnosticCode { code: "634", description: "Incomplete abortion, complete abortion", category: "" },
    DiagnosticCode { code: "635", description: "Therapeutic abortion", category: "" },
    DiagnosticCode { code: "640", description: "Threatened abortion, haemorrhage in early pregnancy", category: "" },
    DiagnosticCode { code: "641", description: "Abruptio placentae, placenta praevia", category: "" },
    DiagnosticCode { code: "642", description: "Pre-eclampsia, eclampsia, toxaemia", category: "" },
    DiagnosticCode { code: "643", description: "Vomiting, hyperemesis gravidarum", category: "" },
    DiagnosticCode { code: "644", description: "False labour, threatened labour", category: "" },
    DiagnosticCode { code: "645", description: "Prolonged pregnancy", category: "" },
    DiagnosticCode { code: "646", description: "Other complications of pregnancy (e.g., vulvitis, vaginitis, cervicitis, pyelitis, cystitis)", category: "" },
    DiagnosticCode { code: "650", description: "Normal delivery, uncomplicated pregnancy", category: "" },
    DiagnosticCode { code: "651", description: "Multiple pregnancy", category: "" },
    DiagnosticCode { code: "652", description: "Unusual position of fetus, malpresentation", category: "" },
    DiagnosticCode { code: "653", description: "Cephalo-pelvic disproportion", category: "" },
    DiagnosticCode { code: "656", description: "Foetal distress", category: "" },
    DiagnosticCode { code: "658", description: "Premature rupture of membrane", category: "" },
    DiagnosticCode { code: "660", description: "Obstructed labour", category: "" },
    DiagnosticCode { code: "661", description: "Uterine inertia", category: "" },
    DiagnosticCode { code: "662", description: "Prolonged labour", category: "" },
    DiagnosticCode { code: "664", description: "Perineal lacerations", category: "" },
    DiagnosticCode { code: "666", description: "Post-Partum haemorrhage", category: "" },
    DiagnosticCode { code: "667", description: "Retained placenta", category: "" },
    DiagnosticCode { code: "669", description: "Delivery with other complications", category: "" },
    DiagnosticCode { code: "671", description: "Post-Partum thrombophlebitis", category: "" },
    DiagnosticCode { code: "675", description: "Post-Partum mastitis or nipple infection", category: "" },
    DiagnosticCode { code: "677", description: "Post-Partum pulmonary", category: "" },
    DiagnosticCode { code: "680", description: "Boil, carbuncle, furunculosis", category: "Infections" },
    DiagnosticCode { code: "682", description: "Cellulitis, abscess", category: "Infections" },
    DiagnosticCode { code: "683", description: "Acute lymphadenitis", category: "Infections" },
    DiagnosticCode { code: "684", description: "Impetigo", category: "Infections" },
    DiagnosticCode { code: "685", description: "Pilonidal cyst or abscess", category: "Infections" },
    DiagnosticCode { code: "686", description: "Pyoderma, pyogenic granuloma, other local infections", category: "Infections" },
    DiagnosticCode { code: "690", description: "Seborrheic dermatitis", category: "Other Inflammatory Conditions" },
    DiagnosticCode { code: "691", description: "Eczema, atopic dermatitis, neurodermatitis", category: "Other Inflammatory Conditions" },
    DiagnosticCode { code: "692", description: "Contact dermatitis", category: "Other Inflammatory Conditions" },
    DiagnosticCode { code: "695", description: "Erythema multiforme, erythema nodosum, acne, rosacea, lupus erythematosus, intertrigo", category: "Other Inflammatory Conditions" },
    DiagnosticCode { code: "696", description: "Psoriasis", category: "Other Inflammatory Conditions" },
    DiagnosticCode { code: "698", description: "Pruritus ani, other itchy conditions", category: "Other Inflammatory Conditions" },
    DiagnosticCode { code: "700", description: "Corns, calluses", category: "Other Diseases of Skin and Subcutaneous Tissue" },
    DiagnosticCode { code: "701", description: "Hyperkeratosis, scleroderma, keloid", category: "Other Diseases of Skin and Subcutaneous Tissue" },
    DiagnosticCode { code: "703", description: "Ingrown nail, onychogryposis", category: "Other Diseases of Skin and Subcutaneous Tissue" },
    DiagnosticCode { code: "704", description: "Alopecia", category: "Other Diseases of Skin and Subcutaneous Tissue" },
    DiagnosticCode { code: "706", description: "Acne, acne vulgaris, sebaceous cyst", category: "Other Diseases of Skin and Subcutaneous Tissue" },
    DiagnosticCode { code: "707", description: "Debcubitus ulcer, bed sore", category: "Other Diseases of Skin and Subcutaneous Tissue" },
    DiagnosticCode { code: "708", description: "Allergic urticaria", category: "Other Diseases of Skin and Subcutaneous Tissue" },
    DiagnosticCode { code: "709", description: "Other disorders of skin and subcutaneous tissue", category: "Other Diseases of Skin and Subcutaneous Tissue" },
    DiagnosticCode { code: "710", description: "Desseminated lupus erythematosus, generalized scleroderma, dermatomyositis, polymostitis", category: "" },
    DiagnosticCode { code: "711", description: "Pyogenic arthritis", category: "" },
    DiagnosticCode { code: "714", description: "Rheumatoid arthritis, Still's disease", category: "" },
    DiagnosticCode { code: "715", description: "Osteoarthritis", category: "" },
    DiagnosticCode { code: "716", description: "Traumatic arthritis", category: "" },
    DiagnosticCode { code: "718", description: "Joint derangement, recurrent dislocation, ankylosis, meniscus or cartilage tear, loose body in joint", category: "" },
    DiagnosticCode { code: "720", description: "Ankylosing spondylitis", category: "" },
    DiagnosticCode { code: "721", description: "Sero-negative Spondyloarthropathies", category: "" },
    DiagnosticCode { code: "722", description: "Intervertebral disc disorders", category: "" },
    DiagnosticCode { code: "724", description: "Lumbar strain, lumbago, coccydynia, sciatica", category: "" },
    DiagnosticCode { code: "725", description: "Polymyalgia rheumatic", category: "" },
    DiagnosticCode { code: "726", description: "Fibromyalgia", category: "" },
    DiagnosticCode { code: "727", description: "Synovitis, tenosynovitis, bursitis, bunion, ganglion", category: "" },
    DiagnosticCode { code: "728", description: "Dupuytren's contracture", category: "" },
    DiagnosticCode { code: "729", description: "Fibrositis, myositis, muscular rheumatism", category: "" },
    DiagnosticCode { code: "730", description: "Osteomyelitis", category: "" },
    DiagnosticCode { code: "731", description: "Osteitis deformans, Paget's disease of bone", category: "" },
    DiagnosticCode { code: "732", description: "Osteochondritis, Legg-Perthes disease, Osgood-Schlatter disease, osteochondritis dissecans", category: "" },
    DiagnosticCode { code: "733", description: "Osteoporosis, spontaneous fracture, other disorders of bone and cartilage", category: "" },
    DiagnosticCode { code: "734", description: "Flat foot, pes planus", category: "" },
    DiagnosticCode { code: "735", description: "Hallux valgus, hallux varus, hammer toe", category: "" },
    DiagnosticCode { code: "737", description: "Scoliosis, kyphosis, lordosis", category: "" },
    DiagnosticCode { code: "739", description: "Other diseases of musculoskeletal system and connective tissue", category: "" },
    DiagnosticCode { code: "741", description: "Spina bifida, with or without hydrocephalus, meningocele, meningomyelocele", category: "" },
    DiagnosticCode { code: "742", description: "Hydrocephalus", category: "" },
    DiagnosticCode { code: "743", description: "Congenital anomalies of eye", category: "" },
    DiagnosticCode { code: "744", description: "Congenital anomalies of ear, face, and neck", category: "" },
    DiagnosticCode { code: "745", description: "Transposition of great vessels, tetralogy of Fallot, ventricular septal defect, atrial septal defect", category: "" },
    DiagnosticCode { code: "746", description: "Other congenital anomalies of heart", category: "" },
    DiagnosticCode { code: "747", description: "Patent ductus arteriosus, coarctation of aorta, pulmonary artery stenosis, other anomalies of circulatory system", category: "" },
    DiagnosticCode { code: "748", description: "Congenital anomalies of nose and respiratory system", category: "" },
    DiagnosticCode { code: "749", description: "Cleft palate, cleft lip", category: "" },
    DiagnosticCode { code: "750", description: "Other congenital anomalies of mouth esophagus, stomach and pylorus", category: "" },
    DiagnosticCode { code: "751", description: "Digestive system", category: "" },
    DiagnosticCode { code: "752", description: "Genital organs", category: "" },
    DiagnosticCode { code: "753", description: "Urinary system", category: "" },
    DiagnosticCode { code: "754", description: "Club foot", category: "" },
    DiagnosticCode { code: "755", description: "Other congenital anomalies of limbs", category: "" },
    DiagnosticCode { code: "756", description: "Other musculoskeletal anomalies", category: "" },
    DiagnosticCode { code: "758", description: "Chromosomal anomalies (e.g., Down's syndrome, other autosomal anomalies, Klinefelter's syndrome, Turner's syndrome, other anomalies of sex chromosomes)", category: "" },
    DiagnosticCode { code: "759", description: "Other congenital anomalies", category: "" },
    DiagnosticCode { code: "762", description: "Compression of umbilical cord, prolapsed cord", category: "" },
    DiagnosticCode { code: "763", description: "Due to complications of labour or delivery", category: "" },
    DiagnosticCode { code: "765", description: "Prematurity, low-birth weight infant", category: "" },
    DiagnosticCode { code: "766", description: "Postmaturity, high-birth weight infant", category: "" },
    DiagnosticCode { code: "767", description: "Birth trauma", category: "" },
    DiagnosticCode { code: "769", description: "Hyaline membrane disease, respiratory distress syndrome", category: "" },
    DiagnosticCode { code: "773", description: "Hemolytic disease of newborn", category: "" },
    DiagnosticCode { code: "777", description: "Perinatal disorders of digestive system", category: "" },
    DiagnosticCode { code: "779", description: "Other conditions of fetus or newborn", category: "" },
    DiagnosticCode { code: "780", description: "Ataxia", category: "Non-specific Abnormal Findings" },
    DiagnosticCode { code: "781", description: "Arthralgia", category: "Non-specific Abnormal Findings" },
    DiagnosticCode { code: "785", description: "Chest pain, tachycardia, syncope, shock, edema, masses", category: "Signs and Symptoms Not Yet Diagnosed" },
    DiagnosticCode { code: "786", description: "Epistaxis, hemoptysis, cough, dyspnea, masses, shortness of breath, hyperventilation, sleep apnea", category: "Signs and Symptoms Not Yet Diagnosed" },
    DiagnosticCode { code: "787", description: "Anorexia, nausea and vomiting, heartburn, dysphagia, hiccough, hematemesis, jaundice, ascites, abdominal pain, melena, masses", category: "Signs and Symptoms Not Yet Diagnosed" },
    DiagnosticCode { code: "788", description: "Renal colic, urinary retention, nocturia, masses", category: "Signs and Symptoms Not Yet Diagnosed" },
    DiagnosticCode { code: "790", description: "Non-specific findings on examination of blood", category: "Non-specific Abnormal Findings" },
    DiagnosticCode { code: "791", description: "Non-specific findings on examination of urine", category: "Non-specific Abnormal Findings" },
    DiagnosticCode { code: "795", description: "Chronic fatigue syndrome", category: "Non-specific Abnormal Findings" },
    DiagnosticCode { code: "796", description: "Other non-specific abnormal findings", category: "Non-specific Abnormal Findings" },
    DiagnosticCode { code: "797", description: "Senility, senescence", category: "Non-specific Abnormal Findings" },
    DiagnosticCode { code: "798", description: "Sudden death, cause unknown", category: "Non-specific Abnormal Findings" },
    DiagnosticCode { code: "799", description: "Other ill-defined conditions", category: "Non-specific Abnormal Findings" },
    DiagnosticCode { code: "802", description: "Facial bones", category: "Fractures and Fracture-dislocations" },
    DiagnosticCode { code: "803", description: "Skull", category: "Fractures and Fracture-dislocations" },
    DiagnosticCode { code: "805", description: "Vertebral column-without spinal cord damage", category: "Fractures and Fracture-dislocations" },
    DiagnosticCode { code: "806", description: "Vertebral column-with spinal cord damage", category: "Fractures and Fracture-dislocations" },
    DiagnosticCode { code: "807", description: "Ribs", category: "Fractures and Fracture-dislocations" },
    DiagnosticCode { code: "808", description: "Pelvis", category: "Fractures and Fracture-dislocations" },
    DiagnosticCode { code: "810", description: "Clavicle", category: "Fractures and Fracture-dislocations" },
    DiagnosticCode { code: "812", description: "Humerus", category: "Fractures and Fracture-dislocations" },
    DiagnosticCode { code: "813", description: "Radius and/or ulna", category: "Fractures and Fracture-dislocations" },
    DiagnosticCode { code: "814", description: "Carpal bones", category: "Fractures and Fracture-dislocations" },
    DiagnosticCode { code: "815", description: "Metacarpals", category: "Fractures and Fracture-dislocations" },
    DiagnosticCode { code: "816", description: "Phalanges-foot or hand", category: "Fractures and Fracture-dislocations" },
    DiagnosticCode { code: "821", description: "Femur", category: "Fractures and Fracture-dislocations" },
    DiagnosticCode { code: "823", description: "Tibia and/or fibula", category: "Fractures and Fracture-dislocations" },
    DiagnosticCode { code: "824", description: "Ankle", category: "Fractures and Fracture-dislocations" },
    DiagnosticCode { code: "829", description: "Other fractures", category: "Fractures and Fracture-dislocations" },
    DiagnosticCode { code: "831", description: "Shoulder", category: "Dislocations" },
    DiagnosticCode { code: "832", description: "Elbow", category: "Dislocations" },
    DiagnosticCode { code: "834", description: "Finger", category: "Dislocations" },
    DiagnosticCode { code: "839", description: "Other dislocations", category: "Dislocations" },
    DiagnosticCode { code: "840", description: "Shoulder, upper arm", category: "Sprains, Strains and Other Trauma" },
    DiagnosticCode { code: "842", description: "Wrist, hand, fingers", category: "Sprains, Strains and Other Trauma" },
    DiagnosticCode { code: "844", description: "Knee, leg", category: "Sprains, Strains and Other Trauma" },
    DiagnosticCode { code: "845", description: "Ankle, foot, toes", category: "Sprains, Strains and Other Trauma" },
    DiagnosticCode { code: "847", description: "Neck, low back, coccyx", category: "Sprains, Strains and Other Trauma" },
    DiagnosticCode { code: "848", description: "Other sprains and strains", category: "Sprains, Strains and Other Trauma" },
    DiagnosticCode { code: "850", description: "Concussion", category: "Sprains, Strains and Other Trauma" },
    DiagnosticCode { code: "854", description: "Other head injuries", category: "Sprains, Strains and Other Trauma" },
    DiagnosticCode { code: "869", description: "Internal injuries to organ(s)", category: "Sprains, Strains and Other Trauma" },
    DiagnosticCode { code: "879", description: "Lacerations, open wounds-except limbs", category: "Sprains, Strains and Other Trauma" },
    DiagnosticCode { code: "884", description: "Lacerations, open wounds, traumatic amputations-upper limb(s)", category: "Sprains, Strains and Other Trauma" },
    DiagnosticCode { code: "894", description: "Lacerations, open wounds, traumatic amputations-lower limb(s)", category: "Sprains, Strains and Other Trauma" },
    DiagnosticCode { code: "895", description: "Family planning, contraceptive advice, advice on sterilization or abortion", category: "Family Planning" },
    DiagnosticCode { code: "896", description: "Immunization-all types", category: "Immunization" },
    DiagnosticCode { code: "897", description: "Economic problems", category: "Social, Marital and Family Problems" },
    DiagnosticCode { code: "898", description: "Marital difficulties", category: "Social, Marital and Family Problems" },
    DiagnosticCode { code: "899", description: "Parent-child problems (e.g., child-abuse, battered child, child neglect)", category: "Social, Marital and Family Problems" },
    DiagnosticCode { code: "900", description: "Problems with aged parents or in-laws", category: "Social, Marital and Family Problems" },
    DiagnosticCode { code: "901", description: "Family disruption, divorce", category: "Social, Marital and Family Problems" },
    DiagnosticCode { code: "902", description: "Educational problems", category: "Social, Marital and Family Problems" },
    DiagnosticCode { code: "904", description: "Social maladjustment", category: "Social, Marital and Family Problems" },
    DiagnosticCode { code: "905", description: "Occupational problems, unemployment, difficulty at work", category: "Social, Marital and Family Problems" },
    DiagnosticCode { code: "906", description: "Legal problems, litigation, imprisonment", category: "Social, Marital and Family Problems" },
    DiagnosticCode { code: "909", description: "Other problems of social adjustment", category: "Social, Marital and Family Problems" },
    DiagnosticCode { code: "916", description: "Well baby care", category: "Other" },
    DiagnosticCode { code: "917", description: "Annual health examination adolescent/adult Well Vision Care", category: "Other" },
    DiagnosticCode { code: "918", description: "Automated Visual Field (AVF) test", category: "Sprains, Strains and Other Trauma" },
    DiagnosticCode { code: "919", description: "Abrasions, bruises, contusions and other superficial injury including non-venomous bites", category: "Sprains, Strains and Other Trauma" },
    DiagnosticCode { code: "930", description: "Foreign body in eye, or other tissues", category: "Sprains, Strains and Other Trauma" },
    DiagnosticCode { code: "949", description: "Burns-thermal or chemical", category: "Sprains, Strains and Other Trauma" },
    DiagnosticCode { code: "959", description: "Other injuries or trauma", category: "Sprains, Strains and Other Trauma" },
    DiagnosticCode { code: "960", description: "Pentavalent (DPT POLIO/ACT HIB)", category: "Immunization" },
    DiagnosticCode { code: "961", description: "DPT Polio", category: "Immunization" },
    DiagnosticCode { code: "962", description: "DT", category: "Immunization" },
    DiagnosticCode { code: "963", description: "MMR (Measles, Mumps, Rubella)", category: "Immunization" },
    DiagnosticCode { code: "964", description: "Hepatitis B", category: "Immunization" },
    DiagnosticCode { code: "965", description: "TD Polio", category: "Immunization" },
    DiagnosticCode { code: "966", description: "TD (Adults and aged 7 years and older)", category: "Immunization" },
    DiagnosticCode { code: "967", description: "Influenza", category: "Immunization" },
    DiagnosticCode { code: "968", description: "Pneumococcal", category: "Immunization" },
    DiagnosticCode { code: "969", description: "Other Immunization-Not Defined", category: "Immunization" },
    DiagnosticCode { code: "972", description: "Recurrent Uveitis", category: "Eye" },
    DiagnosticCode { code: "977", description: "Of drugs and medications-including allergy, overdose, reactions", category: "Adverse Effects" },
    DiagnosticCode { code: "989", description: "Of other chemicals (e.g., lead, pesticides, and venomous bites)", category: "Adverse Effects" },
    DiagnosticCode { code: "994", description: "Of physical factors (e.g., heat, cold, frostbite, pressure)", category: "Adverse Effects" },
    DiagnosticCode { code: "995", description: "Anaphylaxis", category: "Adverse Effects" },
    DiagnosticCode { code: "998", description: "Of surgical and medical care (e.g., wound infection, wound disruption, other iatrogenic disease)", category: "Adverse Effects" },
];

/// Look up a diagnostic code by its 3-digit code string.
pub fn get_diagnostic_code(code: &str) -> Option<&'static DiagnosticCode> {
    DIAGNOSTIC_CODES.iter().find(|c| c.code == code)
}

/// Check if a diagnostic code exists in the database.
pub fn is_valid_diagnostic_code(code: &str) -> bool {
    DIAGNOSTIC_CODES.iter().any(|c| c.code == code)
}

/// Search diagnostic codes by code prefix or description substring (case-insensitive).
/// Returns up to `limit` results.
pub fn search_diagnostic_codes(query: &str, limit: usize) -> Vec<&'static DiagnosticCode> {
    let q = query.to_lowercase();
    let mut results = Vec::new();

    // Exact code match first
    if let Some(exact) = get_diagnostic_code(query) {
        results.push(exact);
    }

    // Code prefix matches
    for dc in &DIAGNOSTIC_CODES {
        if results.len() >= limit {
            break;
        }
        if dc.code.starts_with(query) && !results.iter().any(|r: &&DiagnosticCode| r.code == dc.code) {
            results.push(dc);
        }
    }

    // Description matches
    for dc in &DIAGNOSTIC_CODES {
        if results.len() >= limit {
            break;
        }
        if dc.description.to_lowercase().contains(&q) && !results.iter().any(|r: &&DiagnosticCode| r.code == dc.code) {
            results.push(dc);
        }
    }

    // Category matches
    for dc in &DIAGNOSTIC_CODES {
        if results.len() >= limit {
            break;
        }
        if dc.category.to_lowercase().contains(&q) && !results.iter().any(|r: &&DiagnosticCode| r.code == dc.code) {
            results.push(dc);
        }
    }

    results
}

// ── Common diagnostic codes for prompt guidance ─────────────────────────────

/// A curated list of common diagnostic codes for family practice,
/// used as examples in the LLM extraction prompt.
pub const COMMON_DIAGNOSTIC_CODES: &[(&str, &str)] = &[
    ("250", "Diabetes mellitus"),
    ("272", "Hyperlipidemia"),
    ("300", "Anxiety, neuroses"),
    ("311", "Depression"),
    ("296", "Bipolar disorder"),
    ("309", "Adjustment reaction"),
    ("401", "Essential hypertension"),
    ("410", "Acute myocardial infarction"),
    ("427", "Cardiac arrhythmia"),
    ("436", "Acute cerebrovascular accident"),
    ("460", "Common cold, nasopharyngitis"),
    ("461", "Acute sinusitis"),
    ("466", "Acute bronchitis"),
    ("486", "Pneumonia"),
    ("489", "RSV"),
    ("491", "Chronic bronchitis"),
    ("493", "Asthma"),
    ("496", "COPD"),
    ("530", "GERD, esophageal disease"),
    ("571", "Chronic liver disease"),
    ("574", "Cholelithiasis (gallstones)"),
    ("585", "Chronic kidney disease"),
    ("599", "Urinary tract infection"),
    ("600", "Benign prostatic hyperplasia"),
    ("626", "Disorders of menstruation"),
    ("692", "Dermatitis, eczema"),
    ("715", "Osteoarthritis"),
    ("724", "Back pain"),
    ("726", "Fibromyalgia"),
    ("780", "General symptoms (fatigue, dizziness)"),
    ("785", "Cardiovascular symptoms"),
    ("786", "Respiratory symptoms (cough, dyspnea)"),
    ("787", "GI symptoms (nausea, vomiting, abdominal pain)"),
    ("788", "Urinary symptoms"),
    ("847", "Back sprain/strain"),
    ("916", "Well baby care"),
    ("917", "Annual health examination"),
    ("919", "Abrasions, contusions, superficial injury"),
    ("959", "Other injury or trauma"),
    ("799", "Other ill-defined conditions (use as last resort)"),
];

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_count() {
        assert_eq!(DIAGNOSTIC_CODES.len(), DIAGNOSTIC_CODE_COUNT);
    }

    #[test]
    fn test_get_known_code() {
        let code = get_diagnostic_code("250").expect("250 should exist");
        assert!(code.description.to_lowercase().contains("diabetes"));
    }

    #[test]
    fn test_get_invalid_code() {
        assert!(get_diagnostic_code("000").is_none());
        assert!(get_diagnostic_code("999").is_none());
    }

    #[test]
    fn test_is_valid() {
        assert!(is_valid_diagnostic_code("401"));
        assert!(is_valid_diagnostic_code("917"));
        assert!(!is_valid_diagnostic_code("000"));
    }

    #[test]
    fn test_new_2026_codes() {
        assert!(is_valid_diagnostic_code("308"), "Gender Dysphoria (2026)");
        assert!(is_valid_diagnostic_code("489"), "RSV (2026)");
    }

    #[test]
    fn test_deleted_codes() {
        assert!(!is_valid_diagnostic_code("100"), "Deleted in 2026");
        assert!(!is_valid_diagnostic_code("903"), "Deleted in 2026");
    }

    #[test]
    fn test_search_by_code_prefix() {
        let results = search_diagnostic_codes("25", 10);
        assert!(!results.is_empty());
        assert!(results.iter().any(|r| r.code == "250"));
    }

    #[test]
    fn test_search_by_description() {
        let results = search_diagnostic_codes("diabetes", 10);
        assert!(!results.is_empty());
        assert!(results.iter().any(|r| r.code == "250"));
    }

    #[test]
    fn test_search_by_category() {
        let results = search_diagnostic_codes("hypertensive", 10);
        assert!(!results.is_empty());
        assert!(results.iter().any(|r| r.code == "401"));
    }

    #[test]
    fn test_search_limit() {
        let results = search_diagnostic_codes("a", 5);
        assert!(results.len() <= 5);
    }

    #[test]
    fn test_common_codes_valid() {
        for (code, _) in COMMON_DIAGNOSTIC_CODES {
            assert!(
                is_valid_diagnostic_code(code),
                "Common code {} should be valid",
                code
            );
        }
    }
}

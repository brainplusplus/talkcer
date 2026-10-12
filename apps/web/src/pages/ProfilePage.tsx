import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  getProfile,
  saveProfile,
  uploadCv,
  type CareerEntry,
  type EducationEntry,
  type ProfileInput,
} from "../api";

const EMPTY: ProfileInput = {
  name: "",
  birth_date: "",
  birth_place: "",
  phone: "",
  email: "",
  github: "",
  linkedin: "",
  career: [],
  education: [],
  skills: [],
};

export function ProfilePage() {
  const queryClient = useQueryClient();
  const profile = useQuery({ queryKey: ["profile"], queryFn: getProfile });
  const [form, setForm] = useState<ProfileInput>(EMPTY);
  const [cvNote, setCvNote] = useState("");

  useEffect(() => {
    if (profile.data) {
      setForm({ ...profile.data });
    }
  }, [profile.data]);

  const save = useMutation({
    mutationFn: () => saveProfile(form),
    onSuccess: async () => {
      await queryClient.invalidateQueries({ queryKey: ["profile"] });
    },
  });

  const cv = useMutation({
    mutationFn: (file: File) => uploadCv(file),
    onSuccess: (draft) => {
      if (draft.source === "language_model" && draft.text) {
        const fields = JSON.parse(draft.text) as Partial<ProfileInput>;
        setForm((current) => ({ ...current, ...fields, birth_date: current.birth_date, birth_place: current.birth_place }));
        setCvNote("Filled from the CV. Check it, then save. The file was not kept.");
      } else {
        setCvNote(draft.warning ?? "Could not read the file.");
      }
    },
  });

  function set<K extends keyof ProfileInput>(key: K, value: ProfileInput[K]) {
    setForm((current) => ({ ...current, [key]: value }));
  }

  return (
    <section className="stack wide">
      <h2>Profile</h2>
      <p className="meta">
        This is you, not the assistant. A session can hand it to the model when you tick
        "add profile context", so an interview answer can draw on your real experience.
        Birth date and birth place stay out of that context.
      </p>

      <label>
        Upload a CV to fill the form
        <input
          type="file"
          accept=".pdf,.docx,.txt"
          onChange={(event) => {
            const file = event.target.files?.[0];
            if (file) cv.mutate(file);
          }}
        />
      </label>
      <p className="meta">Uses the model you configured. With no model set, nothing is filled in.</p>
      {cv.isPending && <p className="meta">Reading the CV…</p>}
      {cvNote && <p className="meta">{cvNote}</p>}
      {cv.isError && <p className="error">{(cv.error as Error).message}</p>}

      <Field label="Name" value={form.name} onChange={(value) => set("name", value)} />
      <div className="pair">
        <Field label="Birth date" value={form.birth_date} onChange={(value) => set("birth_date", value)} />
        <Field label="Birth place" value={form.birth_place} onChange={(value) => set("birth_place", value)} />
      </div>
      <div className="pair">
        <Field label="Phone" value={form.phone} onChange={(value) => set("phone", value)} />
        <Field label="Email" value={form.email} onChange={(value) => set("email", value)} />
      </div>
      <Field label="GitHub" value={form.github} onChange={(value) => set("github", value)} />
      <Field label="LinkedIn" value={form.linkedin} onChange={(value) => set("linkedin", value)} />

      <h3>Career history</h3>
      {form.career.map((entry, index) => (
        <CareerRow
          key={index}
          entry={entry}
          onChange={(next) => set("career", replace(form.career, index, next))}
          onRemove={() => set("career", form.career.filter((_, at) => at !== index))}
        />
      ))}
      <button type="button" onClick={() => set("career", [...form.career, blankCareer()])}>
        Add a job
      </button>

      <h3>Education</h3>
      {form.education.map((entry, index) => (
        <EducationRow
          key={index}
          entry={entry}
          onChange={(next) => set("education", replace(form.education, index, next))}
          onRemove={() => set("education", form.education.filter((_, at) => at !== index))}
        />
      ))}
      <button type="button" onClick={() => set("education", [...form.education, blankEducation()])}>
        Add education
      </button>

      <label>
        Skills, one per line
        <textarea
          rows={5}
          value={form.skills.join("\n")}
          onChange={(event) => set("skills", event.target.value.split("\n"))}
        />
      </label>

      <button type="button" className="primary" disabled={save.isPending} onClick={() => save.mutate()}>
        {save.isPending ? "Saving…" : "Save profile"}
      </button>
      {save.isSuccess && <p className="meta">Saved.</p>}
      {save.isError && <p className="error">{(save.error as Error).message}</p>}
    </section>
  );
}

function Field({ label, value, onChange }: { label: string; value: string; onChange: (value: string) => void }) {
  return (
    <label>
      {label}
      <input value={value} onChange={(event) => onChange(event.target.value)} />
    </label>
  );
}

function CareerRow({
  entry,
  onChange,
  onRemove,
}: {
  entry: CareerEntry;
  onChange: (entry: CareerEntry) => void;
  onRemove: () => void;
}) {
  return (
    <div className="card">
      <div className="pair">
        <Field label="Company" value={entry.company} onChange={(company) => onChange({ ...entry, company })} />
        <Field label="Role" value={entry.role} onChange={(role) => onChange({ ...entry, role })} />
      </div>
      <Field label="Period" value={entry.period} onChange={(period) => onChange({ ...entry, period })} />
      <label>
        Summary
        <textarea rows={3} value={entry.summary} onChange={(event) => onChange({ ...entry, summary: event.target.value })} />
      </label>
      <button type="button" onClick={onRemove}>Remove</button>
    </div>
  );
}

function EducationRow({
  entry,
  onChange,
  onRemove,
}: {
  entry: EducationEntry;
  onChange: (entry: EducationEntry) => void;
  onRemove: () => void;
}) {
  return (
    <div className="card">
      <div className="pair">
        <Field label="Institution" value={entry.institution} onChange={(institution) => onChange({ ...entry, institution })} />
        <Field label="Credential" value={entry.credential} onChange={(credential) => onChange({ ...entry, credential })} />
      </div>
      <Field label="Period" value={entry.period} onChange={(period) => onChange({ ...entry, period })} />
      <button type="button" onClick={onRemove}>Remove</button>
    </div>
  );
}

function blankCareer(): CareerEntry {
  return { company: "", role: "", period: "", summary: "" };
}

function blankEducation(): EducationEntry {
  return { institution: "", credential: "", period: "" };
}

function replace<T>(items: T[], index: number, next: T): T[] {
  return items.map((item, at) => (at === index ? next : item));
}

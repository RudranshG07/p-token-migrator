import { readJobs, saveJob, type Job, type Manifest, type SaveJobOptions } from "./migrator.ts";

export interface JobStore {
  list(): Promise<Job[]>;
  find(id: string): Promise<Job | undefined>;
  save(manifest: Manifest, options?: SaveJobOptions): Promise<Job>;
}

export function createFileJobStore(storePath: string): JobStore {
  return {
    list() {
      return readJobs(storePath);
    },
    async find(id: string) {
      const jobs = await readJobs(storePath);
      return jobs.find((job) => job.id === id);
    },
    save(manifest: Manifest, options: SaveJobOptions = {}) {
      return saveJob(manifest, storePath, options);
    }
  };
}

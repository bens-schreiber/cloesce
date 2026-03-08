import { D1Database } from "@cloudflare/workers-types";
import { DataSource, ForeignKey, Model, PrimaryKey, WranglerEnv } from "cloesce/backend";

@WranglerEnv
export class Env {
    db: D1Database;
}

@Model(["GET", "LIST", "SAVE"])
export class Student {
    @PrimaryKey
    id: number;

    @PrimaryKey
    name: string;

    favoriteColor: string;
    courses: StudentCourse[];

    static readonly coursesOrderedDesc: DataSource<Student> = {
        includeTree: {
            courses: {}
        },
        list: (joined) => `
            WITH students AS (${joined()})
            SELECT * FROM students
            WHERE id > ?1
                AND name > ?2
            ORDER BY id DESC, name DESC
            LIMIT ?3
        `,
        listParams: ["LastSeen", "Limit"]
    }
}

@Model(["GET", "LIST", "SAVE"])
export class Course {
    id: number;
    title: string;

    students: StudentCourse[];
}

@Model(["GET", "LIST", "SAVE"])
export class StudentCourse {
    @PrimaryKey
    @ForeignKey<Student>(s => s.id)
    studentId: number;

    @PrimaryKey
    @ForeignKey<Student>(s => s.name)
    studentName: string;
    student: Student;

    @PrimaryKey
    @ForeignKey<Course>(c => c.id)
    courseId: number;
    course: Course;

    static readonly default: DataSource<StudentCourse> = {
        includeTree: {}
    };

    static readonly withStudentCourse: DataSource<StudentCourse> = {
        includeTree: {
            student: { courses: {} },
            course: { students: {} }
        }
    };
}
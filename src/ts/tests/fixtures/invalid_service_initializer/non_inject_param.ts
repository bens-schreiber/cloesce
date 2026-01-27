// @ts-nocheck
// InvalidServiceInitializer
@Service
export class FooService {
  init(invalid: string): void {}
}

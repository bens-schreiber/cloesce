// @ts-nocheck
// InvalidServiceInitializer
@Service
export class FooService {
  async init(): Promise<HttpResult<number>> {}
}

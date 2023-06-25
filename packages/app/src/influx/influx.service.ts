import { Injectable, Logger } from '@nestjs/common'
import { InfluxDB, QueryApi, WriteApi } from '@influxdata/influxdb-client'
import { ConfigService } from '@nestjs/config'

@Injectable()
export class InfluxService {
    private client: InfluxDB
    private writeClient: WriteApi
    private queryClient: QueryApi
    private logger = new Logger(InfluxService.name)

    constructor(private readonly configService: ConfigService) {
        const url = this.configService.get('INFLUXDB_URL')
        const token = this.configService.get('INFLUXDB_TOKEN')
        const org = this.configService.get('INFLUXDB_ORG')
        const bucket = this.configService.get('INFLUXDB_BUCKET')
        this.logger.verbose(
            `InfluxDB config, url: ${url}, token: ${token}, org: ${org}, bucket: ${bucket}`
        )
        if (!url || !token || !org || !bucket)
            throw new Error('Missing InfluxDB configuration')
        this.client = new InfluxDB({ url: url, token: token })
        this.writeClient = this.client.getWriteApi(org, bucket, 's')
        this.queryClient = this.client.getQueryApi(org)
    }
}
